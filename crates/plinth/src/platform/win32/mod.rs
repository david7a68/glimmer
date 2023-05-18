use std::{cell::Cell, ptr::NonNull};

use geometry::{Extent, ScreenPx};
use raw_window_handle::RawWindowHandle;

use windows::{core::Interface, w, Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY};
#[allow(clippy::wildcard_imports)]
use windows::{
    core::PCSTR,
    s,
    Win32::{
        Foundation::{HWND, RECT},
        Graphics::{Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, Direct3D12::*, Dxgi::Common::*},
    },
};

use crate::{
    graphics::{
        Color, ColorSpace, GraphicsConfig, PixelBuffer, PixelBufferRef, PixelFormat, RenderGraph,
        RenderGraphCommand, RenderGraphNodeId, RoundedRectVertex,
    },
    memory::{
        block_allocator::BlockAllocator,
        temp_allocator::{self, FrameMarker},
        HeapOffset,
    },
};

mod dx;
mod queue;
mod surface;

pub use surface::Surface;

use self::queue::SubmissionId;

pub struct Platform {
    dx: dx::Interfaces,
    graphics_queue: queue::Graphics<FrameMarker>,

    white_pixel: Image,

    round_rect_shader: Shader<ShaderConstants>,

    upload_buffer: ID3D12Resource,
    upload_allocator: temp_allocator::Allocator,

    descriptor_heap: DescriptorHeap,
}

impl Platform {
    const UPLOAD_BUFFER_SIZE: u64 = 20 * 1024 * 1024;
    const MAX_TEXTURES: u32 = 1024;

    pub fn new(config: &GraphicsConfig) -> Self {
        let dx = dx::Interfaces::new(config);

        let mut graphics_queue = queue::Graphics::new(&dx);

        let round_rect_shader = create_rounded_rect_shader(&dx);

        let upload_buffer = create_buffer(
            &dx,
            D3D12_HEAP_TYPE_UPLOAD,
            Self::UPLOAD_BUFFER_SIZE,
            D3D12_RESOURCE_STATE_GENERIC_READ,
        );

        let mut upload_allocator = {
            let mut ptr = std::ptr::null_mut();
            unsafe {
                upload_buffer
                    .Map(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }), Some(&mut ptr))
                    .unwrap();
            };

            temp_allocator::Allocator::new(Self::UPLOAD_BUFFER_SIZE, NonNull::new(ptr.cast()))
        };

        let mut descriptor_heap = DescriptorHeap::new(
            &dx,
            D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
            Self::MAX_TEXTURES,
            true,
        );

        let white_pixel = {
            let pixels = PixelBuffer::from_colors(
                // Waiting until we can do `[Color::WHITE.as_rgba8();
                // 4].flatten()`, then we can use `PixelBufferRef::from_bytes`
                [Color::WHITE; 4].as_ref(),
                2,
                PixelFormat::Rgba8,
                ColorSpace::Srgb,
            );

            let (rec, _) = graphics_queue.record(&dx);
            let mut mem = upload_allocator.begin_frame();
            let texture = upload_image(
                &dx,
                &rec.commands,
                &upload_buffer,
                &mut mem,
                &mut descriptor_heap,
                pixels.as_ref(),
            );

            let submit = graphics_queue.submit(rec, mem.finish());
            texture.last_use.set(submit);

            texture
        };

        if dx.is_debug {
            unsafe {
                upload_buffer.SetName(w!("Upload Buffer")).unwrap();
                descriptor_heap
                    .heap
                    .SetName(w!("Texture Descriptor Heap"))
                    .unwrap();
                round_rect_shader
                    .pipeline_state
                    .SetName(w!("Round Rect Shader"))
                    .unwrap();
                round_rect_shader
                    .root_signature
                    .SetName(w!("Round Rect Root Signature"))
                    .unwrap();
                white_pixel.resource.SetName(w!("White Pixel")).unwrap();
            }
        }

        Self {
            dx,
            graphics_queue,
            white_pixel,
            round_rect_shader,
            upload_buffer,
            upload_allocator,
            descriptor_heap,
        }
    }

    pub fn create_surface(&self, window: RawWindowHandle) -> Surface {
        match window {
            RawWindowHandle::Win32(handle) => {
                Surface::new(&self.dx, &self.graphics_queue.queue, HWND(handle.hwnd as _))
            }
            _ => unimplemented!(),
        }
    }

    pub fn destroy_surface(&self, surface: &mut Surface) {
        self.graphics_queue.flush();
        surface.destroy();
    }

    pub fn get_next_image<'a>(&self, surface: &'a mut Surface) -> RenderTarget<'a> {
        let image = surface.get_next_image();
        RenderTarget { image }
    }

    pub fn present(&self, surface: &mut Surface) {
        surface.present();
    }

    pub fn resize(&self, surface: &mut Surface) {
        self.graphics_queue.flush();
        surface.resize(&self.dx);
    }

    pub fn draw(&mut self, target: &RenderTarget, content: &RenderGraph) {
        let target = target.image;

        let (rec, old_marker) = self.graphics_queue.record(&self.dx);
        if let Some(old_marker) = old_marker {
            self.upload_allocator.free_frame(old_marker);
        }

        let mut frame_alloc = self.upload_allocator.begin_frame();

        let (imm_index_view, imm_rect_view) = {
            let upload_address = unsafe { self.upload_buffer.GetGPUVirtualAddress() };

            let index_memory = frame_alloc.upload(&content.imm_indices).unwrap();
            let index_view = D3D12_INDEX_BUFFER_VIEW {
                BufferLocation: upload_address + index_memory.heap_offset,
                SizeInBytes: index_memory.size as u32,
                Format: DXGI_FORMAT_R16_UINT,
            };

            let rect_memory = frame_alloc.upload(&content.imm_rect_vertices).unwrap();
            let rect_view = D3D12_VERTEX_BUFFER_VIEW {
                BufferLocation: upload_address + rect_memory.heap_offset,
                SizeInBytes: rect_memory.size as u32,
                StrideInBytes: std::mem::size_of::<RoundedRectVertex>() as u32,
            };

            (index_view, rect_view)
        };

        let frame_marker = frame_alloc.finish();

        unsafe {
            rec.commands.ResourceBarrier(&[transition_barrier(
                &target.resource,
                D3D12_RESOURCE_STATE_PRESENT,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
            )]);

            if let Ok(dbg) = rec.commands.cast::<ID3D12DebugCommandList>() {
                assert!(dbg
                    .AssertResourceState(&target.resource, 0, D3D12_RESOURCE_STATE_RENDER_TARGET.0)
                    .as_bool());
            }

            rec.commands
                .OMSetRenderTargets(1, Some(&target.rtv.cpu), false, None);

            rec.commands
                .ClearRenderTargetView(target.rtv.cpu, [1.0, 1.0, 1.0, 1.0].as_ptr(), &[]);

            let target_desc = target.resource.GetDesc();

            let constants = ShaderConstants {
                viewport: Extent::new(target_desc.Width as u32, target_desc.Height),
            };

            rec.commands.RSSetViewports(&[D3D12_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: constants.viewport.width as _,
                Height: constants.viewport.height as _,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]);

            rec.commands.RSSetScissorRects(&[RECT {
                left: 0,
                top: 0,
                right: constants.viewport.width.try_into().unwrap(),
                bottom: constants.viewport.height.try_into().unwrap(),
            }]);

            let render_data = RenderData {
                constants,
                white_pixel: &self.white_pixel,
                descriptor_heap: &self.descriptor_heap,
                index_buffer: imm_index_view,
                rect_vertex_buffer: imm_rect_view,
            };

            self.record_render_graph(
                &rec.commands,
                content,
                RenderGraphNodeId::root(),
                &render_data,
            );

            rec.commands.ResourceBarrier(&[transition_barrier(
                &target.resource,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                D3D12_RESOURCE_STATE_PRESENT,
            )]);
        }

        unsafe {
            self.dx.device.GetDeviceRemovedReason().unwrap();
        }

        let fence_value = self.graphics_queue.submit(rec, frame_marker);

        target.last_use.set(fence_value);
    }

    pub fn upload_image(&mut self, pixels: PixelBufferRef) -> Image {
        let (rec, old_marker) = self.graphics_queue.record(&self.dx);

        // Make sure to free old memory before we try to allocate more.
        if let Some(old_marker) = old_marker {
            self.upload_allocator.free_frame(old_marker);
        }

        let mut alloc = self.upload_allocator.begin_frame();

        let image = upload_image(
            &self.dx,
            &rec.commands,
            &self.upload_buffer,
            &mut alloc,
            &mut self.descriptor_heap,
            pixels,
        );

        let submission_id = self.graphics_queue.submit(rec, alloc.finish());

        image.last_use.set(submission_id);

        image
    }

    fn record_render_graph(
        &self,
        command_list: &ID3D12GraphicsCommandList,
        content: &RenderGraph,
        node_id: RenderGraphNodeId,
        data: &RenderData,
    ) {
        'draw: {
            let (first_index, num_indices) = match content.get(node_id) {
                RenderGraphCommand::Root => {
                    assert_eq!(node_id, RenderGraphNodeId::root());
                    break 'draw;
                }
                RenderGraphCommand::DrawRect {
                    first_index,
                    num_indices,
                } => {
                    self.round_rect_shader.bind(
                        command_list,
                        &data.constants,
                        &data.rect_vertex_buffer,
                        &data.index_buffer,
                    );
                    (*first_index, *num_indices)
                }
            };

            unsafe {
                command_list.DrawIndexedInstanced(
                    u32::from(num_indices),
                    1,
                    u32::from(first_index),
                    0,
                    0,
                );
            }
        }

        for child in content.iter_children(node_id) {
            self.record_render_graph(command_list, content, child, data);
        }
    }
}

impl Drop for Platform {
    fn drop(&mut self) {
        self.graphics_queue.flush();
    }
}

pub struct RenderTarget<'a> {
    image: &'a Image,
}

pub struct Image {
    resource: ID3D12Resource,
    last_use: Cell<SubmissionId>,
    rtv: Descriptor,
    srv: Descriptor,
}

fn transition_barrier(
    resource: &ID3D12Resource,
    state_before: D3D12_RESOURCE_STATES,
    state_after: D3D12_RESOURCE_STATES,
) -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: windows::core::ManuallyDrop::new(resource),
                StateBefore: state_before,
                StateAfter: state_after,
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
            }),
        },
    }
}

struct ShaderConstants {
    viewport: Extent<u32, ScreenPx>,
}

impl PushConstants for ShaderConstants {
    unsafe fn write(&self, command_list: &ID3D12GraphicsCommandList) {
        command_list.SetGraphicsRoot32BitConstants(
            0,
            2,
            [self.viewport.width, self.viewport.height].as_ptr().cast(),
            0,
        );
    }
}

fn create_rounded_rect_shader(dx: &dx::Interfaces) -> Shader<ShaderConstants> {
    Shader::new(
        dx,
        include_bytes!(concat!(env!("OUT_DIR"), "/rect_vs.cso")),
        include_bytes!(concat!(env!("OUT_DIR"), "/rect_ps.cso")),
        DXGI_FORMAT_R16G16B16A16_FLOAT,
        D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
        &[
            vertex_input(s!("POSITION"), 0, DXGI_FORMAT_R32G32_FLOAT, 0),
            vertex_input(s!("RECT_SIZE"), 0, DXGI_FORMAT_R32G32_FLOAT, 0),
            vertex_input(s!("RECT_CENTER"), 0, DXGI_FORMAT_R32G32_FLOAT, 0),
            vertex_input(s!("OUTER_RADIUS"), 0, DXGI_FORMAT_R32G32B32A32_FLOAT, 0),
            vertex_input(s!("INNER_RADIUS"), 0, DXGI_FORMAT_R32G32B32A32_FLOAT, 0),
            vertex_input(s!("COLOR\0"), 0, DXGI_FORMAT_R32G32B32A32_FLOAT, 0),
        ],
    )
}

trait PushConstants {
    unsafe fn write(&self, command_list: &ID3D12GraphicsCommandList);
}

struct Shader<Constants: PushConstants> {
    root_signature: ID3D12RootSignature,
    pipeline_state: ID3D12PipelineState,
    primitive_topology: D3D_PRIMITIVE_TOPOLOGY,
    phantom: std::marker::PhantomData<Constants>,
}

impl<Constants: PushConstants> Shader<Constants> {
    fn new(
        dx: &dx::Interfaces,
        vertex_shader: &[u8],
        pixel_shader: &[u8],
        format: DXGI_FORMAT,
        topology: D3D_PRIMITIVE_TOPOLOGY,
        input: &[D3D12_INPUT_ELEMENT_DESC],
    ) -> Self {
        let root_signature = unsafe { dx.device.CreateRootSignature(0, vertex_shader) }.unwrap();

        let mut blend_targets = [D3D12_RENDER_TARGET_BLEND_DESC::default(); 8];

        // Blend with premultiplied alpha
        blend_targets[0] = D3D12_RENDER_TARGET_BLEND_DESC {
            BlendEnable: true.into(),
            LogicOpEnable: false.into(),
            SrcBlend: D3D12_BLEND_ONE,
            DestBlend: D3D12_BLEND_INV_SRC_ALPHA,
            BlendOp: D3D12_BLEND_OP_ADD,
            SrcBlendAlpha: D3D12_BLEND_ONE,
            DestBlendAlpha: D3D12_BLEND_ONE,
            BlendOpAlpha: D3D12_BLEND_OP_ADD,
            LogicOp: D3D12_LOGIC_OP_NOOP,
            RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
        };

        let mut render_target_formats = [DXGI_FORMAT_UNKNOWN; 8];
        render_target_formats[0] = format;

        let pipeline_info = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: windows::core::ManuallyDrop::new(&root_signature),
            VS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: vertex_shader.as_ptr().cast(),
                BytecodeLength: vertex_shader.len(),
            },
            PS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: pixel_shader.as_ptr().cast(),
                BytecodeLength: pixel_shader.len(),
            },
            BlendState: D3D12_BLEND_DESC {
                AlphaToCoverageEnable: false.into(),
                IndependentBlendEnable: false.into(),
                RenderTarget: blend_targets,
            },
            SampleMask: u32::MAX,
            RasterizerState: D3D12_RASTERIZER_DESC {
                FillMode: D3D12_FILL_MODE_SOLID,
                CullMode: D3D12_CULL_MODE_BACK,
                FrontCounterClockwise: false.into(),
                DepthBias: 0,
                DepthBiasClamp: 0.0,
                SlopeScaledDepthBias: 0.0,
                DepthClipEnable: false.into(),
                MultisampleEnable: false.into(),
                AntialiasedLineEnable: false.into(),
                ForcedSampleCount: 0,
                ConservativeRaster: D3D12_CONSERVATIVE_RASTERIZATION_MODE_OFF,
            },
            DepthStencilState: D3D12_DEPTH_STENCIL_DESC {
                DepthEnable: false.into(),
                DepthWriteMask: D3D12_DEPTH_WRITE_MASK_ALL,
                DepthFunc: D3D12_COMPARISON_FUNC_LESS,
                StencilEnable: false.into(),
                StencilReadMask: 0xFF,
                StencilWriteMask: 0xFF,
                FrontFace: D3D12_DEPTH_STENCILOP_DESC {
                    StencilFailOp: D3D12_STENCIL_OP_KEEP,
                    StencilDepthFailOp: D3D12_STENCIL_OP_KEEP,
                    StencilPassOp: D3D12_STENCIL_OP_KEEP,
                    StencilFunc: D3D12_COMPARISON_FUNC_ALWAYS,
                },
                BackFace: D3D12_DEPTH_STENCILOP_DESC {
                    StencilFailOp: D3D12_STENCIL_OP_KEEP,
                    StencilDepthFailOp: D3D12_STENCIL_OP_KEEP,
                    StencilPassOp: D3D12_STENCIL_OP_KEEP,
                    StencilFunc: D3D12_COMPARISON_FUNC_ALWAYS,
                },
            },
            InputLayout: D3D12_INPUT_LAYOUT_DESC {
                pInputElementDescs: input.as_ptr(),
                NumElements: u32::try_from(input.len()).unwrap(),
            },
            PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            NumRenderTargets: 1,
            RTVFormats: render_target_formats,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            NodeMask: 0,
            Flags: D3D12_PIPELINE_STATE_FLAG_NONE,
            ..Default::default()
        };

        let pipeline_state =
            unsafe { dx.device.CreateGraphicsPipelineState(&pipeline_info) }.unwrap();

        Self {
            root_signature,
            pipeline_state,
            primitive_topology: topology,
            phantom: std::marker::PhantomData,
        }
    }

    fn bind(
        &self,
        command_list: &ID3D12GraphicsCommandList,
        constants: &Constants,
        vertices: &D3D12_VERTEX_BUFFER_VIEW,
        indices: &D3D12_INDEX_BUFFER_VIEW,
    ) {
        unsafe {
            command_list.SetPipelineState(&self.pipeline_state);
            command_list.SetGraphicsRootSignature(&self.root_signature);
            command_list.IASetPrimitiveTopology(self.primitive_topology);
            command_list.IASetVertexBuffers(0, Some(&[*vertices]));
            command_list.IASetIndexBuffer(Some(indices));
            constants.write(command_list);
        }
    }
}

struct RenderData<'a> {
    constants: ShaderConstants,
    white_pixel: &'a Image,
    descriptor_heap: &'a DescriptorHeap,
    index_buffer: D3D12_INDEX_BUFFER_VIEW,
    rect_vertex_buffer: D3D12_VERTEX_BUFFER_VIEW,
}

fn create_buffer(
    dx: &dx::Interfaces,
    heap: D3D12_HEAP_TYPE,
    size: u64,
    initial_state: D3D12_RESOURCE_STATES,
) -> ID3D12Resource {
    let mut buffer = None;
    unsafe {
        dx.device.CreateCommittedResource(
            &D3D12_HEAP_PROPERTIES {
                Type: heap,
                CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
                MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
                CreationNodeMask: 0,
                VisibleNodeMask: 0,
            },
            D3D12_HEAP_FLAG_NONE, // set automatically by CreateCommitedResource
            &D3D12_RESOURCE_DESC {
                Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                Alignment: 0, // default: 64k
                Width: size,
                Height: 1,
                DepthOrArraySize: 1,
                MipLevels: 1,
                Format: DXGI_FORMAT_UNKNOWN,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                Flags: D3D12_RESOURCE_FLAG_NONE,
            },
            initial_state,
            None,
            &mut buffer,
        )
    }
    .unwrap();
    buffer.unwrap()
}

fn vertex_input(
    name: PCSTR,
    index: u32,
    format: DXGI_FORMAT,
    slot: u32,
) -> D3D12_INPUT_ELEMENT_DESC {
    D3D12_INPUT_ELEMENT_DESC {
        SemanticName: name,
        SemanticIndex: index,
        Format: format,
        InputSlot: slot,
        AlignedByteOffset: D3D12_APPEND_ALIGNED_ELEMENT,
        InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
        InstanceDataStepRate: 0,
    }
}

fn upload_image(
    dx: &dx::Interfaces,
    command_list: &ID3D12GraphicsCommandList,
    upload_heap: &ID3D12Resource,
    allocator: &mut temp_allocator::FrameAllocator,
    descriptor_heap: &mut DescriptorHeap,
    pixels: PixelBufferRef,
) -> Image {
    // To avoid recalculating
    let pixels_height = pixels.height();

    let format = match pixels.format() {
        PixelFormat::Rgba8 => DXGI_FORMAT_R8G8B8A8_UNORM,
    };

    let image = {
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Alignment: 0,
            Width: pixels.width().into(),
            Height: pixels_height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
            Flags: D3D12_RESOURCE_FLAG_NONE,
        };

        let mut image: Option<ID3D12Resource> = None;
        unsafe {
            dx.device
                .CreateCommittedResource(
                    &D3D12_HEAP_PROPERTIES {
                        Type: D3D12_HEAP_TYPE_DEFAULT,
                        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
                        MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
                        CreationNodeMask: 0,
                        VisibleNodeMask: 0,
                    },
                    D3D12_HEAP_FLAG_NONE,
                    &desc,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    None,
                    &mut image,
                )
                .unwrap();
        }

        image.unwrap()
    };

    let footprint = D3D12_SUBRESOURCE_FOOTPRINT {
        Format: format,
        Width: pixels.width(),
        Height: pixels_height,
        Depth: 1,
        RowPitch: next_multiple_of_u32(
            pixels.width() * (pixels.format().bytes_per_pixel() as u32),
            D3D12_TEXTURE_DATA_PITCH_ALIGNMENT,
        ),
    };

    let (mem, bytes) = allocator
        .allocate(
            u64::from(footprint.RowPitch) * u64::from(pixels_height),
            D3D12_TEXTURE_DATA_PLACEMENT_ALIGNMENT.into(),
        )
        .expect("upload allocator at capacity");

    let bytes = bytes.expect("upload allocator with no host memory?");

    for (i, row) in pixels.rows().enumerate() {
        let offset = footprint.RowPitch as usize * i;
        unsafe {
            std::ptr::copy_nonoverlapping(row.as_ptr(), bytes.as_mut_ptr().add(offset), row.len());
        }
    }

    let placed_desc = D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
        Offset: mem.heap_offset,
        Footprint: footprint,
    };

    unsafe {
        command_list.CopyTextureRegion(
            &D3D12_TEXTURE_COPY_LOCATION {
                pResource: windows::core::ManuallyDrop::new(&image),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    SubresourceIndex: 0,
                },
            },
            0,
            0,
            0,
            &D3D12_TEXTURE_COPY_LOCATION {
                pResource: windows::core::ManuallyDrop::new(upload_heap),
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: placed_desc,
                },
            },
            None,
        );

        command_list.ResourceBarrier(&[transition_barrier(
            &image,
            D3D12_RESOURCE_STATE_COPY_DEST,
            D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
        )]);
    }

    let srv = {
        let desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: format,
            ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
            Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                Texture2D: D3D12_TEX2D_SRV {
                    MostDetailedMip: 0,
                    MipLevels: 1,
                    PlaneSlice: 0,
                    ResourceMinLODClamp: 0.0,
                },
            },
        };

        descriptor_heap.create_shader_resource_view(dx, &image, &desc)
    };

    Image {
        resource: image,
        last_use: Cell::new(SubmissionId::default()),
        rtv: Descriptor::default(),
        srv,
    }
}

#[derive(Debug, Default)]
struct Descriptor {
    cpu: D3D12_CPU_DESCRIPTOR_HANDLE,
    gpu: D3D12_GPU_DESCRIPTOR_HANDLE,
}

struct DescriptorHeap {
    heap: ID3D12DescriptorHeap,
    kind: D3D12_DESCRIPTOR_HEAP_TYPE,
    cpu_start: u64,
    gpu_start: u64,
    allocator: BlockAllocator,
}

impl DescriptorHeap {
    fn new(
        dx: &dx::Interfaces,
        kind: D3D12_DESCRIPTOR_HEAP_TYPE,
        max_descriptors: u32,
        is_shader_visible: bool,
    ) -> Self {
        let desc = D3D12_DESCRIPTOR_HEAP_DESC {
            Type: kind,
            NumDescriptors: max_descriptors,
            Flags: if is_shader_visible {
                D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE
            } else {
                D3D12_DESCRIPTOR_HEAP_FLAG_NONE
            },
            NodeMask: 0,
        };

        let heap: ID3D12DescriptorHeap = unsafe { dx.device.CreateDescriptorHeap(&desc) }.unwrap();

        let cpu_start = unsafe { heap.GetCPUDescriptorHandleForHeapStart().ptr } as u64;
        let gpu_start = if is_shader_visible {
            unsafe { heap.GetGPUDescriptorHandleForHeapStart().ptr }
        } else {
            0
        };

        let allocator = BlockAllocator::new(
            HeapOffset(u64::from(unsafe {
                dx.device.GetDescriptorHandleIncrementSize(kind)
            })),
            max_descriptors,
        );

        Self {
            heap,
            kind,
            cpu_start,
            gpu_start,
            allocator,
        }
    }

    fn free(&mut self, descriptor: Descriptor) {
        let offset =
            unsafe { descriptor.cpu.ptr - self.heap.GetCPUDescriptorHandleForHeapStart().ptr };
        self.allocator.free(HeapOffset(offset as u64));
    }

    fn create_shader_resource_view(
        &mut self,
        dx: &dx::Interfaces,
        resource: &ID3D12Resource,
        desc: &D3D12_SHADER_RESOURCE_VIEW_DESC,
    ) -> Descriptor {
        debug_assert_eq!(self.kind, D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);

        let offset = self.allocator.allocate().unwrap();
        let handle = {
            Descriptor {
                cpu: D3D12_CPU_DESCRIPTOR_HANDLE {
                    ptr: (self.cpu_start + offset.0) as usize,
                },
                gpu: D3D12_GPU_DESCRIPTOR_HANDLE {
                    ptr: self.gpu_start + offset.0,
                },
            }
        };

        unsafe {
            dx.device
                .CreateShaderResourceView(resource, Some(desc), handle.cpu);
        }
        handle
    }

    fn create_render_target_view(
        &mut self,
        dx: &dx::Interfaces,
        resource: &ID3D12Resource,
        desc: Option<*const D3D12_RENDER_TARGET_VIEW_DESC>,
    ) -> Descriptor {
        debug_assert_eq!(self.kind, D3D12_DESCRIPTOR_HEAP_TYPE_RTV);
        let offset = self.allocator.allocate().unwrap();
        let handle = {
            Descriptor {
                cpu: D3D12_CPU_DESCRIPTOR_HANDLE {
                    ptr: (self.cpu_start + offset.0) as usize,
                },
                gpu: D3D12_GPU_DESCRIPTOR_HANDLE::default(),
            }
        };

        unsafe {
            dx.device.CreateRenderTargetView(resource, desc, handle.cpu);
        }
        handle
    }
}

// Waiting until next_multiple_of stabilizes in std (https://github.com/rust-lang/rust/issues/88581).
fn next_multiple_of_u32(value: u32, multiple: u32) -> u32 {
    match value % multiple {
        0 => value,
        remainder => value
            .checked_add(multiple - remainder)
            .expect("int overflow"),
    }
}
