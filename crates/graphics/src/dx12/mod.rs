use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use raw_window_handle::RawWindowHandle;
use smallvec::SmallVec;
use windows::Win32::{Foundation::HWND, Graphics::Direct3D12::*};

use crate::{render_graph::RenderGraph, GraphicsConfig};

mod dx;
mod graphics;
mod surface;

pub use surface::{Surface, SurfaceImage};

pub struct GraphicsContext {
    dx: Rc<dx::Interfaces>,
    graphics_queue: Rc<RefCell<graphics::Queue>>,
}

impl GraphicsContext {
    pub fn new(config: &GraphicsConfig) -> Self {
        let dx = dx::Interfaces::new(config);

        let graphics_queue = graphics::Queue::new(&dx);

        Self {
            dx: Rc::new(dx),
            graphics_queue: Rc::new(RefCell::new(graphics_queue)),
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
                // Note (2022-12-21): This disagrees with the samples, and
                // involves a clone that is destroyed immediately upon
                // submission to a command list. IDK why this is the case.
                pResource: windows::core::ManuallyDrop::new(resource),
                StateBefore: state_before,
                StateAfter: state_after,
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
            }),
        },
    }
}
