use raw_window_handle::RawWindowHandle;
use windows::{
    core::Interface,
    Win32::{
        Foundation::{BOOL, HANDLE, HWND},
        Graphics::{
            Direct3D::D3D_FEATURE_LEVEL_12_0,
            Direct3D12::{
                D3D12CreateDevice, D3D12GetDebugInterface, ID3D12CommandAllocator,
                ID3D12CommandQueue, ID3D12Debug, ID3D12DescriptorHeap, ID3D12Device, ID3D12Fence,
                ID3D12Resource, D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_QUEUE_DESC,
                D3D12_CPU_DESCRIPTOR_HANDLE, D3D12_DESCRIPTOR_HEAP_DESC,
                D3D12_DESCRIPTOR_HEAP_FLAG_NONE, D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                D3D12_FENCE_FLAG_NONE,
            },
            Dxgi::{
                Common::{
                    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT, DXGI_FORMAT_R16G16B16A16_FLOAT,
                    DXGI_SAMPLE_DESC,
                },
                CreateDXGIFactory2, IDXGIAdapter, IDXGIFactory6, IDXGISwapChain3,
                DXGI_CREATE_FACTORY_DEBUG, DXGI_FEATURE_PRESENT_ALLOW_TEARING,
                DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE, DXGI_GPU_PREFERENCE_MINIMUM_POWER,
                DXGI_GPU_PREFERENCE_UNSPECIFIED, DXGI_MWA_NO_ALT_ENTER, DXGI_PRESENT_ALLOW_TEARING,
                DXGI_SCALING_NONE, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
                DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING,
                DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT, DXGI_SWAP_EFFECT_FLIP_DISCARD,
                DXGI_USAGE_RENDER_TARGET_OUTPUT,
            },
        },
        System::Threading::{CreateEventW, WaitForSingleObjectEx},
    },
};

use crate::{GraphicsConfig, PowerPreference};

pub struct GraphicsContext {
    dx: Interfaces,
    fence: ID3D12Fence,
    fence_value: u64,
    fence_event: HANDLE,
    command_queue: ID3D12CommandQueue,
    command_allocator: ID3D12CommandAllocator,
}

impl GraphicsContext {
    pub fn new(config: &GraphicsConfig) -> Self {
        let dx = Interfaces::new(config);

        let fence = unsafe { dx.device.CreateFence(0, D3D12_FENCE_FLAG_NONE) }.unwrap();
        let fence_event = unsafe { CreateEventW(None, false, false, None) }.unwrap();

        let command_queue = unsafe {
            dx.device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                ..Default::default()
            })
        }
        .unwrap();

        let command_allocator = unsafe {
            dx.device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
        }
        .unwrap();

        Self {
            dx,
            fence,
            fence_value: 1,
            fence_event,
            command_queue,
            command_allocator,
        }
    }

    pub fn create_surface(&self, window: RawWindowHandle) -> Surface {
        match window {
            RawWindowHandle::Win32(handle) => {
                Surface::new(&self.dx, &self.command_queue, HWND(handle.hwnd as _))
            }
            _ => unimplemented!(),
        }
    }
}

impl Drop for GraphicsContext {
    fn drop(&mut self) {
        // no-op
    }
}

/*
Notes on DXGI surfaces:

 - May need to set the DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING for displays with
   variable refresh rates.
 - To optimize scrolling, it might make sense to use the
   DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL swap effect. Would need a lot of additional
   infrastructure to support this though.
*/

/// A `Surface` controls the acquisition and presentation of images to its
/// associated window.
pub struct Surface {
    // Use swapchain3 for color space support
    flags: DXGI_SWAP_CHAIN_FLAG,
    swapchain: IDXGISwapChain3,
    image_index: u32,
    waitable_object: HANDLE,
    render_target_descriptor_heap: ID3D12DescriptorHeap,
}

impl Surface {
    const BUFFER_COUNT: u32 = 2;
    const FORMAT: DXGI_FORMAT = DXGI_FORMAT_R16G16B16A16_FLOAT;

    fn new(dx: &Interfaces, queue: &ID3D12CommandQueue, window: HWND) -> Self {
        // Setting this flag lets us limit the number of frames in the present
        // queue. If the application renders faster than the display can present
        // them, the application will block until the display catches up.
        let mut flags = DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT;

        if dx.allows_tearing {
            // Try to support tearing if the display supports it. This is to
            // allow the use of displays with variable-refresh-rate (VRR). In
            // doing so, it is important to perform frame pacing with another
            // method.
            flags.0 |= DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING.0;
        }

        let swapchain: IDXGISwapChain3 = unsafe {
            dx.gi.CreateSwapChainForHwnd(
                queue,
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
                    // Note: Discard the back buffer contents after presenting.
                    SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
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

        let render_target_descriptor_heap: ID3D12DescriptorHeap = unsafe {
            dx.device.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                NumDescriptors: Self::BUFFER_COUNT,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                NodeMask: 0,
            })
        }
        .unwrap();

        let render_targets: [ID3D12Resource; 2] = [
            unsafe { swapchain.GetBuffer(0) }.unwrap(),
            unsafe { swapchain.GetBuffer(1) }.unwrap(),
        ];

        unsafe {
            let heap_start = render_target_descriptor_heap.GetCPUDescriptorHandleForHeapStart();

            let heap_increment = dx
                .device
                .GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_RTV)
                as usize;

            dx.device
                .CreateRenderTargetView(&render_targets[0], None, heap_start);

            dx.device.CreateRenderTargetView(
                &render_targets[1],
                None, // default render target view
                D3D12_CPU_DESCRIPTOR_HANDLE {
                    ptr: heap_start.ptr + heap_increment,
                },
            );
        }

        Self {
            flags,
            swapchain,
            image_index: 0,
            waitable_object,
            render_target_descriptor_heap,
        }
    }

    pub fn resize(&mut self) {
        unsafe {
            self.swapchain.ResizeBuffers(
                Self::BUFFER_COUNT,
                0, // automatically match the size of the window
                0, // automatically match the size of the window
                Self::FORMAT,
                self.flags.0 as u32,
            )
        }
        .unwrap();
    }

    pub fn get_next_image(&mut self) -> SurfaceImage {
        unsafe { WaitForSingleObjectEx(self.waitable_object, u32::MAX, false) };
        self.image_index = unsafe { self.swapchain.GetCurrentBackBufferIndex() };
        SurfaceImage { surface: self }
    }
}

pub struct SurfaceImage<'a> {
    surface: &'a Surface,
}

impl SurfaceImage<'_> {
    pub fn present(self) {
        // must check if the window is in windowed mode

        let present_flags = if (self.surface.flags.0 & DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING.0) != 0 {
            DXGI_PRESENT_ALLOW_TEARING
        } else {
            0
        };

        // Present interval must be 0 (present immediately) for tearing to work
        unsafe { self.surface.swapchain.Present(0, present_flags) }.unwrap();
    }
}

struct Interfaces {
    gi: IDXGIFactory6,
    device: ID3D12Device,
    allows_tearing: bool,
}

impl Interfaces {
    pub fn new(config: &GraphicsConfig) -> Self {
        // Use IDXGIFactory6 for power preferece selection
        let gi: IDXGIFactory6 = {
            let flags = if config.debug_mode {
                DXGI_CREATE_FACTORY_DEBUG
            } else {
                0
            };

            unsafe { CreateDXGIFactory2(flags) }.unwrap()
        };

        let allows_tearing = {
            let mut value: BOOL = BOOL::default();
            unsafe {
                gi.CheckFeatureSupport(
                    DXGI_FEATURE_PRESENT_ALLOW_TEARING,
                    &mut value as *mut BOOL as *mut _,
                    std::mem::size_of::<BOOL>() as u32,
                )
            }
            .unwrap();
            value.into()
        };

        let power_preference = match config.power_preference {
            PowerPreference::DontCare => DXGI_GPU_PREFERENCE_UNSPECIFIED,
            PowerPreference::LowPower => DXGI_GPU_PREFERENCE_MINIMUM_POWER,
            PowerPreference::HiPower => DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
        };

        let adapter: IDXGIAdapter = unsafe { gi.EnumAdapterByGpuPreference(0, power_preference) }
            .or_else(|_| unsafe { gi.EnumWarpAdapter() })
            .unwrap();

        if config.debug_mode {
            let mut dx_debug: Option<ID3D12Debug> = None;
            unsafe { D3D12GetDebugInterface(&mut dx_debug) }.unwrap();
            unsafe { dx_debug.unwrap().EnableDebugLayer() };
        }

        let mut device: Option<ID3D12Device> = None;
        unsafe { D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_12_0, &mut device) }.unwrap();

        Self {
            gi,
            device: device.unwrap(),
            allows_tearing,
        }
    }
}
