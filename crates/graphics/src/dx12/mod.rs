use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use raw_window_handle::RawWindowHandle;
use smallvec::SmallVec;
use windows::{
    s,
    Win32::{
        Foundation::HWND,
        Graphics::{Direct3D12::*, Dxgi::Common::*},
    },
};

use crate::{render_graph::RenderGraph, GraphicsConfig};

mod dx;
mod graphics;
mod surface;
mod temp_allocator;

pub use surface::{Surface, SurfaceImage};

pub struct GraphicsContext {
    dx: Rc<dx::Interfaces>,
    graphics_queue: Rc<RefCell<graphics::Queue>>,
    ui_shader: UiShader,
}

impl GraphicsContext {
    pub fn new(config: &GraphicsConfig) -> Self {
        let dx = dx::Interfaces::new(config);

        let graphics_queue = graphics::Queue::new(&dx);

        let ui_shader = UiShader::new(&dx);

        Self {
            dx: Rc::new(dx),
            graphics_queue: Rc::new(RefCell::new(graphics_queue)),
            ui_shader,
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

    pub fn draw(&mut self, target: &Image, _content: &RenderGraph) {
        let mut graphics = self.graphics_queue.borrow_mut();
        graphics.release_completed_submissions();

        let allocator = graphics.get_command_allocator(&self.dx);
        let command_list = graphics.get_command_list(&self.dx, &allocator);

        unsafe {
            command_list.ResourceBarrier(&[transition_barrier(
                &target.resource,
                D3D12_RESOURCE_STATE_PRESENT,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
            )]);

            command_list.ClearRenderTargetView(target.rtv, [0.5, 0.5, 0.5, 1.0].as_ptr(), &[]);

            // record draw commands!

            // upload render graph immediate geometry

            // draw

            command_list.ResourceBarrier(&[transition_barrier(
                &target.resource,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                D3D12_RESOURCE_STATE_PRESENT,
            )]);
        }

        let fence_value = graphics.submit(allocator, &[command_list], SmallVec::default());
        target.last_use.set(fence_value);
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

struct UiShader {
    root_signature: ID3D12RootSignature,
    pipeline_state: ID3D12PipelineState,
}

impl UiShader {
    const UI_VERTEX_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ui_vs.cso"));
    const UI_PIXEL_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ui_ps.cso"));

    fn new(dx: &dx::Interfaces) -> Self {
        let root_signature =
            unsafe { dx.device.CreateRootSignature(0, Self::UI_VERTEX_SHADER) }.unwrap();

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
                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                InputSlot: 0,
                AlignedByteOffset: D3D12_APPEND_ALIGNED_ELEMENT,
                InputSlotClass: D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
        ];

        let mut blend_targets = [Default::default(); 8];
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
        render_target_formats[0] = DXGI_FORMAT_R8G8B8A8_UNORM;

        let pipeline_info = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            pRootSignature: windows::core::ManuallyDrop::new(&root_signature),
            VS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: Self::UI_VERTEX_SHADER.as_ptr().cast(),
                BytecodeLength: Self::UI_VERTEX_SHADER.len(),
            },
            PS: D3D12_SHADER_BYTECODE {
                pShaderBytecode: Self::UI_PIXEL_SHADER.as_ptr().cast(),
                BytecodeLength: Self::UI_PIXEL_SHADER.len(),
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
                FrontCounterClockwise: true.into(),
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
                StencilReadMask: D3D12_DEFAULT_STENCIL_READ_MASK as u8,
                StencilWriteMask: D3D12_DEFAULT_STENCIL_WRITE_MASK as u8,
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
                pInputElementDescs: input_elements.as_ptr(),
                NumElements: input_elements.len() as _,
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
}
