use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use windows::{
    core::Interface,
    w,
    Win32::{
        Foundation::{CloseHandle, HANDLE, HWND},
        Graphics::{
            Direct3D12::*,
            Dxgi::{Common::*, *},
        },
        System::Threading::WaitForSingleObject,
    },
};

use super::{dx, graphics, Image};

/// A `Surface` controls the acquisition and presentation of images to its
/// associated window.
pub struct Surface {
    dx: Rc<dx::Interfaces>,
    graphics_queue: Rc<RefCell<graphics::Queue>>,
    flags: DXGI_SWAP_CHAIN_FLAG,
    // Use swapchain3 for color space support
    swapchain: IDXGISwapChain3,
    image_index: u32,
    frame_counter: Cell<u64>,
    render_targets: [Option<Image>; Surface::BUFFER_COUNT as usize],
    waitable_object: HANDLE,
    rtv_heap: ID3D12DescriptorHeap,
}

impl Surface {
    /// Double-buffered swapchain.
    const BUFFER_COUNT: u32 = 2;
    // Default swapchain format. Windows will clamp the format to the 0-1 range
    // on SDR displays.
    const FORMAT: DXGI_FORMAT = DXGI_FORMAT_R16G16B16A16_FLOAT;

    pub fn new(dx: Rc<dx::Interfaces>, queue: Rc<RefCell<graphics::Queue>>, window: HWND) -> Self {
        // Setting this flag lets us limit the number of frames in the present
        // queue. If the application renders faster than the display can present
        // them, the application will block until the display catches up.
        let flags = DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT;

        let swapchain: IDXGISwapChain3 = unsafe {
            dx.gi.CreateSwapChainForHwnd(
                &queue.borrow().queue,
                window,
                &DXGI_SWAP_CHAIN_DESC1 {
                    Width: 0,  // automatically match the size of the window
                    Height: 0, // automatically match the size of the window
                    // Note: For HDR support, further work is needed
                    // (2022-12-19).
                    Format: Self::FORMAT,
                    Stereo: false.into(),
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                    BufferCount: Self::BUFFER_COUNT,
                    // Note: DXGI_SCALING_NONE is not supported on Windows 7.
                    // May want to adjust accordingly.
                    Scaling: DXGI_SCALING_NONE,
                    // Note: DISCARD has higher performance than SEQUENTIAL,
                    // since the DWM can overwrite parts of the image with
                    // overlapped windows instead of copying it into its own
                    // memory. However, it may make sense to use SEQUENTIAL if
                    // partial swapchain updates are needed.
                    SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                    // We don't care about transparent windows at the moment.
                    AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                    Flags: flags.0 as u32,
                },
                None,
                None,
            )
        }
        .unwrap()
        .cast()
        .unwrap();

        // Disable fullscreen transitions
        unsafe { dx.gi.MakeWindowAssociation(window, DXGI_MWA_NO_ALT_ENTER) }.unwrap();

        let waitable_object = unsafe { swapchain.GetFrameLatencyWaitableObject() };

        let rtv_heap: ID3D12DescriptorHeap = unsafe {
            dx.device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                NumDescriptors: Self::BUFFER_COUNT,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                NodeMask: 0,
            })
        }
        .unwrap();

        let [a, b] = Self::get_render_targets(&dx, &swapchain, &rtv_heap);

        Self {
            dx,
            graphics_queue: queue,
            flags,
            swapchain,
            image_index: 0,
            frame_counter: Cell::new(0),
            render_targets: [Some(a), Some(b)],
            waitable_object,
            rtv_heap,
        }
    }

    pub fn resize(&mut self) {
        // make sure that the render targets aren't currently in use
        let mut graphics_queue = self.graphics_queue.borrow_mut();
        graphics_queue.flush();

        let _ = self.render_targets[0].take().unwrap();
        let _ = self.render_targets[1].take().unwrap();

        unsafe {
            self.swapchain.ResizeBuffers(
                0,
                0, // automatically match the size of the window
                0, // automatically match the size of the window
                DXGI_FORMAT_UNKNOWN,
                self.flags.0 as u32,
            )
        }
        .unwrap();

        let [a, b] = Self::get_render_targets(&self.dx, &self.swapchain, &self.rtv_heap);
        self.render_targets = [Some(a), Some(b)];
    }

    /// Retrieves the next image in the swap chain.
    ///
    /// This function will block until the next image is available.
    pub fn get_next_image(&mut self) -> SurfaceImage {
        // block until the next image is available
        //
        // NOTE: should this instead be done just before presenting???
        unsafe { WaitForSingleObject(self.waitable_object, u32::MAX) }
            .ok()
            .unwrap();

        self.image_index = unsafe { self.swapchain.GetCurrentBackBufferIndex() };

        SurfaceImage { surface: self }
    }

    fn get_render_targets(
        dx: &dx::Interfaces,
        swapchain: &IDXGISwapChain3,
        rtv_heap: &ID3D12DescriptorHeap,
    ) -> [Image; 2] {
        unsafe {
            let heap_start = rtv_heap.GetCPUDescriptorHandleForHeapStart();
            let heap_increment = dx
                .device
                .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV)
                as usize;

            let buffer0: ID3D12Resource = swapchain.GetBuffer(0).unwrap();
            let rtv0 = heap_start;
            dx.device.CreateRenderTargetView(&buffer0, None, heap_start);

            let buffer1: ID3D12Resource = swapchain.GetBuffer(1).unwrap();
            let rtv1 = D3D12_CPU_DESCRIPTOR_HANDLE {
                ptr: heap_start.ptr + heap_increment,
            };
            dx.device.CreateRenderTargetView(
                &buffer1,
                None, // default render target view
                D3D12_CPU_DESCRIPTOR_HANDLE {
                    ptr: heap_start.ptr + heap_increment,
                },
            );

            #[cfg(debug_assertions)]
            if dx.is_debug {
                buffer0.SetName(w!("Swapchain Buffer 0")).unwrap();
                buffer1.SetName(w!("Swapchain Buffer 1")).unwrap();
            }

            [
                Image {
                    resource: buffer0,
                    last_use: Cell::new(0),
                    rtv: rtv0,
                },
                Image {
                    resource: buffer1,
                    last_use: Cell::new(0),
                    rtv: rtv1,
                },
            ]
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.graphics_queue.borrow_mut().flush();
        unsafe { CloseHandle(self.waitable_object) }.ok().unwrap();
    }
}

pub struct SurfaceImage<'a> {
    surface: &'a Surface,
}

impl SurfaceImage<'_> {
    /// Presents the image to the surface.
    pub fn present(self) {
        // must check if the window is in windowed mode

        self.surface
            .frame_counter
            .set(self.surface.frame_counter.get() + 1);
        // We assume that the window is not typically in borderless fullscreen,
        // and so use a presentation interval of 1 (VSync).
        unsafe { self.surface.swapchain.Present(1, 0) }.unwrap();
    }

    pub fn get_image(&self) -> &Image {
        self.surface.render_targets[self.surface.image_index as usize]
            .as_ref()
            .unwrap()
    }
}
