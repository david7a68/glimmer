use raw_window_handle::RawWindowHandle;
use windows::{
    core::Interface,
    Win32::{
        Foundation::{HANDLE, HWND},
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
                DXGI_CREATE_FACTORY_DEBUG, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
                DXGI_GPU_PREFERENCE_MINIMUM_POWER, DXGI_GPU_PREFERENCE_UNSPECIFIED,
                DXGI_MWA_NO_ALT_ENTER, DXGI_SCALING_NONE, DXGI_SWAP_CHAIN_DESC1,
                DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT,
                DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
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

/// A `Surface` controls the acquisition and presentation of images to its
/// associated window.
pub struct Surface {
    // Use swapchain3 for color space support
    flags: DXGI_SWAP_CHAIN_FLAG,
    swapchain: IDXGISwapChain3,
    image_index: u32,
    render_targets: [ID3D12Resource; Surface::BUFFER_COUNT as usize],
    waitable_object: HANDLE,
    render_target_descriptor_heap: ID3D12DescriptorHeap,
}

impl Surface {
    /// Double-buffered swapchain.
    const BUFFER_COUNT: u32 = 2;
    // Default swapchain format. Windows will clamp the format to the 0-1 range
    // on SDR displays.
    const FORMAT: DXGI_FORMAT = DXGI_FORMAT_R16G16B16A16_FLOAT;

    fn new(dx: &Interfaces, queue: &ID3D12CommandQueue, window: HWND) -> Self {
        // Setting this flag lets us limit the number of frames in the present
        // queue. If the application renders faster than the display can present
        // them, the application will block until the display catches up.
        let flags = DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT;

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
            render_targets,
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

        self.render_targets = [
            unsafe { self.swapchain.GetBuffer(0) }.unwrap(),
            unsafe { self.swapchain.GetBuffer(1) }.unwrap(),
        ];
    }

    /// Retrieves the next image in the swap chain.
    ///
    /// This function will block until the next image is available.
    pub fn get_next_image(&mut self) -> SurfaceImage {
        // block until the next image is available
        //
        // NOTE: should this instead be done just before presenting???
        unsafe { WaitForSingleObjectEx(self.waitable_object, u32::MAX, false) };

        self.image_index = unsafe { self.swapchain.GetCurrentBackBufferIndex() };
        SurfaceImage { surface: self }
    }
}

pub struct SurfaceImage<'a> {
    surface: &'a Surface,
}

impl SurfaceImage<'_> {
    /// Presents the image to the surface.
    pub fn present(self) {
        // must check if the window is in windowed mode

        // We assume that the window is not typically in borderless fullscreen,
        // and so use a presentation interval of 1 (VSync).
        unsafe { self.surface.swapchain.Present(1, 0) }.unwrap();
    }
}

struct Interfaces {
    gi: IDXGIFactory6,
    device: ID3D12Device,
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
        }
    }
}
