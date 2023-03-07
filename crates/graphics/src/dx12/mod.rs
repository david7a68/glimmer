//! # DX12 backend

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    ptr::NonNull,
    rc::Rc,
};

use geometry::{Extent, ScreenSpace};
use raw_window_handle::RawWindowHandle;
use smallvec::SmallVec;

use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY;
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
    memory::{
        block_allocator::BlockAllocator,
        temp_allocator::{self, FrameMarker},
        HeapOffset,
    },
    render_graph::{RenderGraph, RenderGraphCommand},
    GraphicsConfig, RenderGraphNodeId, RoundedRectVertex, Vertex,
};

mod dx;
mod queue;
mod surface;

pub use surface::{Surface, SurfaceImage};

struct Frame {
    barriers: SmallVec<[D3D12_RESOURCE_BARRIER; 2]>,
    command_list: ID3D12GraphicsCommandList,
    command_allocator: ID3D12CommandAllocator,
}

impl Frame {
    fn new(dx: &dx::Interfaces) -> Self {
        let allocator = unsafe {
            dx.device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
        }
        .unwrap();

        let command_list: ID3D12GraphicsCommandList = unsafe {
            dx.device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
        }
        .unwrap();

        Self {
            barriers: SmallVec::new(),
            command_list,
            command_allocator: allocator,
        }
    }
}

struct FrameInFlight {
    frame: Frame,
    fence_value: u64,
    alloc_marker: FrameMarker,
}

pub struct GraphicsContext {
    dx: Rc<dx::Interfaces>,
    graphics_queue: Rc<RefCell<queue::Graphics>>,

    white_pixel: Option<Image>,

    polygon_shader: Shader<ShaderConstants>,
    round_rect_shader: Shader<ShaderConstants>,

    upload_buffer: ID3D12Resource,
    upload_allocator: temp_allocator::Allocator,

    descriptor_heap: ID3D12DescriptorHeap,
    descriptor_allocator: BlockAllocator<{ Self::MAX_TEXTURES as usize }>,

    unused_frames: Vec<Frame>,
    frames_in_flight: VecDeque<FrameInFlight>,
}

impl GraphicsContext {
    const UPLOAD_BUFFER_SIZE: u64 = 1024 * 1024;
    const MAX_TEXTURES: u32 = 1024;

    pub fn new(config: &GraphicsConfig) -> Self {
        let dx = dx::Interfaces::new(config);

        let graphics_queue = queue::Graphics::new(&dx);

        let polygon_shader = create_polygon_shader(&dx);
        let round_rect_shader = create_rounded_rect_shader(&dx);

        let upload_buffer = create_buffer(
            &dx,
            D3D12_HEAP_TYPE_UPLOAD,
            Self::UPLOAD_BUFFER_SIZE,
            D3D12_RESOURCE_STATE_GENERIC_READ,
        );

        let upload_allocator = {
            let mut ptr = std::ptr::null_mut();
            unsafe {
                upload_buffer
                    .Map(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }), Some(&mut ptr))
                    .unwrap();
            };

            temp_allocator::Allocator::new(Self::UPLOAD_BUFFER_SIZE, NonNull::new(ptr.cast()))
        };

        let descriptor_heap = {
            let heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                NumDescriptors: Self::MAX_TEXTURES,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                NodeMask: 0,
            };

            unsafe { dx.device.CreateDescriptorHeap(&heap_desc) }.unwrap()
        };

        let descriptor_allocator = {
            let descriptor_size = unsafe {
                dx.device
                    .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV)
            } as u64;

            BlockAllocator::<{ Self::MAX_TEXTURES as usize }>::new(HeapOffset(descriptor_size))
        };

        Self {
            dx: Rc::new(dx),
            graphics_queue: Rc::new(RefCell::new(graphics_queue)),
            white_pixel: None,
            polygon_shader,
            round_rect_shader,
            upload_buffer,
            upload_allocator,
            descriptor_heap,
            descriptor_allocator,
            unused_frames: Vec::new(),
            frames_in_flight: VecDeque::new(),
        }
    }

    pub fn create_surface(&self, window: RawWindowHandle) -> Surface {
        match window {
            RawWindowHandle::Win32(handle) => Surface::new(
                self.dx.clone(),
                self.graphics_queue.clone(),
                HWND(handle.hwnd as _),
            ),
            _ => unimplemented!(),
        }
    }

    pub fn draw(&mut self, target: &Image, content: &RenderGraph) {
        let frame = self.begin_frame();
        let mut frame_alloc = self.upload_allocator.begin_frame();

        let (imm_vertex_view, imm_index_view, imm_rect_view) = {
            let vertex_memory = frame_alloc.upload(&content.imm_vertices).unwrap();
            let vertex_view = vertex_buffer_view::<Vertex>(&self.upload_buffer, &vertex_memory);

            let index_memory = frame_alloc.upload(&content.imm_indices).unwrap();
            let index_view = D3D12_INDEX_BUFFER_VIEW {
                BufferLocation: unsafe { self.upload_buffer.GetGPUVirtualAddress() }
                    + index_memory.heap_offset,
                SizeInBytes: index_memory.size as u32,
                Format: DXGI_FORMAT_R16_UINT,
            };

            let rect_memory = frame_alloc.upload(&content.imm_rect_vertices).unwrap();
            let rect_view =
                vertex_buffer_view::<RoundedRectVertex>(&self.upload_buffer, &rect_memory);

            (vertex_view, index_view, rect_view)
        };

        let _white_pixel = self.white_pixel.get_or_insert_with(|| {
            let resource = create_white_pixel(
                &self.dx,
                &frame.command_list,
                &self.upload_buffer,
                &mut frame_alloc,
            );

            let desc = D3D12_SHADER_RESOURCE_VIEW_DESC {
                Format: DXGI_FORMAT_R8G8B8A8_UINT,
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

            let srv_offset = self.descriptor_allocator.allocate().unwrap();

            let srv = unsafe {
                let srv = D3D12_CPU_DESCRIPTOR_HANDLE {
                    ptr: self
                        .descriptor_heap
                        .GetCPUDescriptorHandleForHeapStart()
                        .ptr
                        + srv_offset.0 as usize,
                };
                self.dx
                    .device
                    .CreateShaderResourceView(&resource, Some(&desc), srv);
                srv
            };

            Image {
                resource,
                last_use: Cell::new(0),
                rtv: D3D12_CPU_DESCRIPTOR_HANDLE::default(),
                srv,
            }
        });

        let frame_marker = frame_alloc.finish();

        unsafe {
            frame.command_list.ResourceBarrier(&[transition_barrier(
                &target.resource,
                D3D12_RESOURCE_STATE_PRESENT,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
            )]);

            frame
                .command_list
                .OMSetRenderTargets(1, Some(&target.rtv), false, None);

            frame.command_list.ClearRenderTargetView(
                target.rtv,
                [1.0, 1.0, 1.0, 1.0].as_ptr(),
                &[],
            );

            let target_desc = target.resource.GetDesc();

            let constants = ShaderConstants {
                viewport: Extent::new(target_desc.Width as u32, target_desc.Height),
            };

            frame.command_list.RSSetViewports(&[D3D12_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: constants.viewport.width as _,
                Height: constants.viewport.height as _,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]);

            frame.command_list.RSSetScissorRects(&[RECT {
                left: 0,
                top: 0,
                right: constants.viewport.width.try_into().unwrap(),
                bottom: constants.viewport.height.try_into().unwrap(),
            }]);

            self.record_render_graph(
                &frame.command_list,
                content,
                RenderGraphNodeId::root(),
                &constants,
                &imm_rect_view,
                &imm_index_view,
                &imm_vertex_view,
            );

            frame.command_list.ResourceBarrier(&[transition_barrier(
                &target.resource,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                D3D12_RESOURCE_STATE_PRESENT,
            )]);
        }

        unsafe {
            self.dx.device.GetDeviceRemovedReason().unwrap();
        }

        let fence_value = self.submit_frame(frame, frame_marker);
        target.last_use.set(fence_value);
    }

    fn begin_frame(&mut self) -> Frame {
        let graphics_queue = self.graphics_queue.borrow();
        let num_complete = self
            .frames_in_flight
            .iter()
            .take_while(|frame| graphics_queue.is_complete(frame.fence_value))
            .count();

        for FrameInFlight {
            mut frame,
            alloc_marker: frame_marker,
            ..
        } in self.frames_in_flight.drain(..num_complete)
        {
            for mut barrier in frame.barriers.drain(..) {
                match barrier.Type {
                    D3D12_RESOURCE_BARRIER_TYPE_TRANSITION => unsafe {
                        std::mem::ManuallyDrop::drop(&mut barrier.Anonymous.Transition)
                    },
                    _ => unimplemented!(),
                }
            }

            unsafe {
                frame.command_allocator.Reset().unwrap();
                frame
                    .command_list
                    .Reset(&frame.command_allocator, None)
                    .unwrap();
            }

            self.upload_allocator.free_frame(frame_marker);
            self.unused_frames.push(frame);
        }

        self.unused_frames
            .pop()
            .unwrap_or_else(|| Frame::new(&self.dx))
    }

    fn submit_frame(&mut self, frame: Frame, alloc_marker: FrameMarker) -> u64 {
        let mut graphics = self.graphics_queue.borrow_mut();

        let fence_value = graphics.submit(&frame.command_list);

        self.frames_in_flight.push_back(FrameInFlight {
            frame,
            fence_value,
            alloc_marker,
        });

        fence_value
    }

    #[allow(clippy::too_many_arguments)]
    fn record_render_graph(
        &self,
        command_list: &ID3D12GraphicsCommandList,
        content: &RenderGraph,
        node_id: RenderGraphNodeId,
        constants: &ShaderConstants,
        imm_rect_view: &D3D12_VERTEX_BUFFER_VIEW,
        imm_index_view: &D3D12_INDEX_BUFFER_VIEW,
        imm_vertex_view: &D3D12_VERTEX_BUFFER_VIEW,
    ) {
        'draw: {
            let (first_index, num_indices, shader, vertex_view) = match content.get(node_id) {
                RenderGraphCommand::Root => {
                    assert_eq!(node_id, RenderGraphNodeId::root());
                    break 'draw;
                }
                RenderGraphCommand::DrawImmediate {
                    first_index,
                    num_indices,
                } => (
                    *first_index,
                    *num_indices,
                    &self.polygon_shader,
                    imm_vertex_view,
                ),
                RenderGraphCommand::DrawRect {
                    first_index,
                    num_indices,
                } => (
                    *first_index,
                    *num_indices,
                    &self.round_rect_shader,
                    imm_rect_view,
                ),
            };

            shader.bind(command_list, constants, vertex_view, imm_index_view);

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
            self.record_render_graph(
                command_list,
                content,
                child,
                constants,
                imm_rect_view,
                imm_index_view,
                imm_vertex_view,
            );
        }
    }
}

impl Drop for GraphicsContext {
    fn drop(&mut self) {
        self.graphics_queue.borrow_mut().flush();
    }
}

pub struct Image {
    resource: ID3D12Resource,
    last_use: Cell<u64>,
    rtv: D3D12_CPU_DESCRIPTOR_HANDLE,
    srv: D3D12_CPU_DESCRIPTOR_HANDLE,
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
    viewport: Extent<u32, ScreenSpace>,
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

fn create_polygon_shader(dx: &dx::Interfaces) -> Shader<ShaderConstants> {
    Shader::new(
        dx,
        include_bytes!(concat!(env!("OUT_DIR"), "/polygon_vs.cso")),
        include_bytes!(concat!(env!("OUT_DIR"), "/polygon_ps.cso")),
        DXGI_FORMAT_R16G16B16A16_FLOAT,
        D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
        &[
            vertex_input(s!("POSITION"), 0, DXGI_FORMAT_R32G32_FLOAT, 0),
            vertex_input(s!("TEXCOORD"), 0, DXGI_FORMAT_R32G32_FLOAT, 0),
            vertex_input(s!("COLOR"), 0, DXGI_FORMAT_R32G32B32A32_FLOAT, 0),
        ],
    )
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
        vbuf: &D3D12_VERTEX_BUFFER_VIEW,
        ibuf: &D3D12_INDEX_BUFFER_VIEW,
    ) {
        unsafe {
            command_list.SetPipelineState(&self.pipeline_state);
            command_list.SetGraphicsRootSignature(&self.root_signature);
            command_list.IASetPrimitiveTopology(self.primitive_topology);
            command_list.IASetVertexBuffers(0, Some(&[*vbuf]));
            command_list.IASetIndexBuffer(Some(ibuf));
            constants.write(command_list);
        }
    }
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

fn vertex_buffer_view<T: Copy>(
    heap: &ID3D12Resource,
    allocation: &temp_allocator::Allocation,
) -> D3D12_VERTEX_BUFFER_VIEW {
    D3D12_VERTEX_BUFFER_VIEW {
        BufferLocation: unsafe { heap.GetGPUVirtualAddress() } + allocation.heap_offset,
        SizeInBytes: u32::try_from(allocation.size).unwrap(),
        StrideInBytes: u32::try_from(std::mem::size_of::<T>()).unwrap(),
    }
}

fn create_white_pixel(
    dx: &dx::Interfaces,
    command_list: &ID3D12GraphicsCommandList,
    upload_heap: &ID3D12Resource,
    upload_memory: &mut temp_allocator::FrameAllocator,
) -> ID3D12Resource {
    let desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        Alignment: 0,
        Width: 1,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UINT,
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
    let image = image.unwrap();

    let upload_allocation = upload_memory.upload(&[255, 255, 255, 255]).unwrap();

    let placed_desc = D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
        Offset: upload_allocation.heap_offset,
        Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
            Format: DXGI_FORMAT_R8G8B8A8_UINT,
            Width: 1,
            Height: 1,
            Depth: 1,
            RowPitch: D3D12_TEXTURE_DATA_PITCH_ALIGNMENT,
        },
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

    image
}
