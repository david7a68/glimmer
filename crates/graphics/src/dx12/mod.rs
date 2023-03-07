use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

use geometry::{Extent, ScreenSpace};
use raw_window_handle::RawWindowHandle;
use smallvec::SmallVec;
#[allow(clippy::wildcard_imports)]
use windows::{
    s,
    Win32::{
        Foundation::{HWND, RECT},
        Graphics::{Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, Direct3D12::*, Dxgi::Common::*},
    },
};

use crate::{render_graph::RenderGraph, GraphicsConfig, RenderGraphNodeId, Vertex};

mod dx;
mod graphics;
mod surface;
mod temp_allocator;

pub use surface::{Surface, SurfaceImage};

use self::temp_allocator::FrameMarker;

struct Frame {
    barriers: SmallVec<[D3D12_RESOURCE_BARRIER; 2]>,
    command_list: ID3D12GraphicsCommandList,
    command_allocator: ID3D12CommandAllocator,
}

struct FrameInFlight {
    frame: Frame,
    fence_value: u64,
    alloc_marker: FrameMarker,
}

pub struct GraphicsContext {
    dx: Rc<dx::Interfaces>,
    graphics_queue: Rc<RefCell<graphics::Queue>>,
    ui_shader: Polygon,

    upload_ptr: *mut std::ffi::c_void,
    upload_buffer: ID3D12Resource,
    upload_allocator: temp_allocator::Allocator,

    unused_frames: Vec<Frame>,
    frames_in_flight: VecDeque<FrameInFlight>,
}

impl GraphicsContext {
    const UPLOAD_BUFFER_SIZE: u64 = 1024;

    pub fn new(config: &GraphicsConfig) -> Self {
        let dx = dx::Interfaces::new(config);

        let graphics_queue = graphics::Queue::new(&dx);

        let ui_shader = Polygon::new(&dx);

        // create upload buffer
        let upload_buffer: ID3D12Resource = unsafe {
            let mut buffer = None;
            dx.device
                .CreateCommittedResource(
                    &D3D12_HEAP_PROPERTIES {
                        Type: D3D12_HEAP_TYPE_UPLOAD,
                        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
                        MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
                        CreationNodeMask: 0,
                        VisibleNodeMask: 0,
                    },
                    D3D12_HEAP_FLAG_NONE, // set automatically by CreateCommitedResource
                    &D3D12_RESOURCE_DESC {
                        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                        Alignment: 0, // default: 64k
                        Width: Self::UPLOAD_BUFFER_SIZE,
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
                    D3D12_RESOURCE_STATE_GENERIC_READ,
                    None,
                    &mut buffer,
                )
                .unwrap();
            buffer.unwrap()
        };

        let upload_ptr = unsafe {
            // persistently mapped pointer
            let mut ptr = std::ptr::null_mut();
            upload_buffer
                .Map(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }), Some(&mut ptr))
                .unwrap();
            ptr
        };

        let upload_allocator = temp_allocator::Allocator::new(Self::UPLOAD_BUFFER_SIZE);

        Self {
            dx: Rc::new(dx),
            graphics_queue: Rc::new(RefCell::new(graphics_queue)),
            ui_shader,
            upload_ptr,
            upload_buffer,
            upload_allocator,
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

        let (frame_marker, imm_vertex_view, imm_index_view) = {
            let mut frame_alloc = self.upload_allocator.begin_frame();

            let vertex_memory = frame_alloc
                .allocate(
                    (content.imm_vertices.len() * std::mem::size_of::<Vertex>()) as u64,
                    std::mem::align_of::<Vertex>() as u64,
                )
                .expect("temporary memory allocation failed, todo: handle this gracefully");

            unsafe {
                std::slice::from_raw_parts_mut(
                    self.upload_ptr
                        .add(vertex_memory.heap_offset as usize)
                        .cast(),
                    content.imm_vertices.len(),
                )
                .copy_from_slice(&content.imm_vertices);
            }

            let vertex_view = D3D12_VERTEX_BUFFER_VIEW {
                BufferLocation: unsafe { self.upload_buffer.GetGPUVirtualAddress() }
                    + vertex_memory.heap_offset,
                SizeInBytes: vertex_memory.size as u32,
                StrideInBytes: std::mem::size_of::<Vertex>() as u32,
            };

            let index_memory = frame_alloc
                .allocate(
                    (content.imm_indices.len() * std::mem::size_of::<u16>()) as u64,
                    std::mem::align_of::<u16>() as u64,
                )
                .expect("temporary memory allocation failed, todo: handle this gracefully");

            unsafe {
                std::slice::from_raw_parts_mut(
                    self.upload_ptr
                        .add(index_memory.heap_offset as usize)
                        .cast(),
                    content.imm_indices.len(),
                )
                .copy_from_slice(&content.imm_indices);
            }

            let index_view = D3D12_INDEX_BUFFER_VIEW {
                BufferLocation: unsafe { self.upload_buffer.GetGPUVirtualAddress() }
                    + index_memory.heap_offset,
                SizeInBytes: index_memory.size as u32,
                Format: DXGI_FORMAT_R16_UINT,
            };

            (frame_alloc.finish(), vertex_view, index_view)
        };

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
                [0.5, 0.5, 0.5, 1.0].as_ptr(),
                &[],
            );

            let target_desc = target.resource.GetDesc();

            let constants =
                ShaderConstants::new(Extent::new(target_desc.Width as u32, target_desc.Height));

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
                &imm_vertex_view,
                &imm_index_view,
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
        self.reclaim_completed_frames();

        self.unused_frames.pop().unwrap_or_else(|| {
            let allocator = unsafe {
                self.dx
                    .device
                    .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
            }
            .unwrap();

            let command_list = unsafe {
                self.dx.device.CreateCommandList(
                    0,
                    D3D12_COMMAND_LIST_TYPE_DIRECT,
                    &allocator,
                    None,
                )
            }
            .unwrap();

            Frame {
                barriers: SmallVec::new(),
                command_list,
                command_allocator: allocator,
            }
        })
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

    fn reclaim_completed_frames(&mut self) {
        let graphics_queue = self.graphics_queue.borrow();

        let mut i = 0;
        for frame in &self.frames_in_flight {
            if graphics_queue.is_complete(frame.fence_value) {
                i += 1;
            } else {
                break;
            }
        }

        for FrameInFlight {
            mut frame,
            alloc_marker: frame_marker,
            ..
        } in self.frames_in_flight.drain(..i)
        {
            for mut barrier in frame.barriers.drain(..) {
                if barrier.Type == D3D12_RESOURCE_BARRIER_TYPE_TRANSITION {
                    unsafe { std::mem::ManuallyDrop::drop(&mut barrier.Anonymous.Transition) };
                }
            }

            self.upload_allocator.free_frame(frame_marker);

            unsafe {
                frame.command_allocator.Reset().unwrap();
                frame
                    .command_list
                    .Reset(&frame.command_allocator, None)
                    .unwrap();
                self.unused_frames.push(frame);
            }
        }
    }

    fn record_render_graph(
        &self,
        command_list: &ID3D12GraphicsCommandList,
        content: &RenderGraph,
        node: RenderGraphNodeId,
        constants: &ShaderConstants,
        imm_vertex_view: &D3D12_VERTEX_BUFFER_VIEW,
        imm_index_view: &D3D12_INDEX_BUFFER_VIEW,
    ) {
        use crate::render_graph::RenderGraphCommand;

        match content.get(node) {
            RenderGraphCommand::Root => assert_eq!(node, RenderGraphNodeId::root()),
            RenderGraphCommand::DrawImmediate {
                first_index,
                num_indices,
            } => unsafe {
                self.ui_shader.bind(command_list, constants);
                command_list.IASetVertexBuffers(0, Some(&[*imm_vertex_view]));
                command_list.IASetIndexBuffer(Some(imm_index_view));

                command_list.DrawIndexedInstanced(
                    u32::from(*num_indices),
                    1,
                    u32::from(*first_index),
                    0,
                    0,
                );
            },
        }

        for child in content.iter_children(node) {
            self.record_render_graph(
                command_list,
                content,
                child,
                constants,
                imm_vertex_view,
                imm_index_view,
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

impl ShaderConstants {
    fn new(viewport: Extent<u32, ScreenSpace>) -> Self {
        Self { viewport }
    }

    fn write(&self, command_list: &ID3D12GraphicsCommandList) {
        unsafe {
            command_list.SetGraphicsRoot32BitConstants(
                0,
                2,
                [self.viewport.width, self.viewport.height].as_ptr().cast(),
                0,
            );
        }
    }
}

struct Polygon {
    shader: Shader,
}

impl Polygon {
    const UI_VERTEX_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/polygon_vs.cso"));
    const UI_PIXEL_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/polygon_ps.cso"));

    #[allow(clippy::too_many_lines)]
    fn new(dx: &dx::Interfaces) -> Self {
        let input_elements = [
            D3D12_INPUT_ELEMENT_DESC {
                SemanticName: s!("POSITION"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 0,
                InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D12_INPUT_ELEMENT_DESC {
                SemanticName: s!("COLOR\0"),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: D3D12_APPEND_ALIGNED_ELEMENT,
                InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
        ];

        let shader = Shader::new(
            dx,
            Self::UI_VERTEX_SHADER,
            Self::UI_PIXEL_SHADER,
            None,
            DXGI_FORMAT_R16G16B16A16_FLOAT,
            &input_elements,
        );

        Self { shader }
    }

    fn bind(&self, command_list: &ID3D12GraphicsCommandList, constants: &ShaderConstants) {
        self.shader.bind(command_list);
        constants.write(command_list);
        unsafe { command_list.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST) };
    }
}

struct Shader {
    root_signature: ID3D12RootSignature,
    pipeline_state: ID3D12PipelineState,
}

impl Shader {
    fn new(
        dx: &dx::Interfaces,
        vertex_shader: &[u8],
        pixel_shader: &[u8],
        geometry_shader: Option<&[u8]>,
        format: DXGI_FORMAT,
        input: &[D3D12_INPUT_ELEMENT_DESC],
    ) -> Shader {
        let root_signature = unsafe { dx.device.CreateRootSignature(0, vertex_shader) }.unwrap();

        let mut blend_targets = [D3D12_RENDER_TARGET_BLEND_DESC::default(); 8];
        blend_targets[0] = D3D12_RENDER_TARGET_BLEND_DESC {
            BlendEnable: true.into(),
            LogicOpEnable: false.into(),
            SrcBlend: D3D12_BLEND_ONE,
            DestBlend: D3D12_BLEND_ZERO,
            BlendOp: D3D12_BLEND_OP_ADD,
            SrcBlendAlpha: D3D12_BLEND_ONE,
            DestBlendAlpha: D3D12_BLEND_ZERO,
            BlendOpAlpha: D3D12_BLEND_OP_ADD,
            LogicOp: D3D12_LOGIC_OP_NOOP,
            RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
        };

        let mut render_target_formats = [DXGI_FORMAT_UNKNOWN; 8];
        render_target_formats[0] = format;

        let gs = {
            let mut gs = D3D12_SHADER_BYTECODE {
                pShaderBytecode: std::ptr::null(),
                BytecodeLength: 0,
            };
            if let Some(geometry_shader) = geometry_shader {
                gs = D3D12_SHADER_BYTECODE {
                    pShaderBytecode: geometry_shader.as_ptr().cast(),
                    BytecodeLength: geometry_shader.len(),
                };
            }
            gs
        };

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
            GS: gs,
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
        }
    }

    fn bind(&self, command_list: &ID3D12GraphicsCommandList) {
        unsafe {
            command_list.SetPipelineState(&self.pipeline_state);
            command_list.SetGraphicsRootSignature(&self.root_signature);
        }
    }
}
