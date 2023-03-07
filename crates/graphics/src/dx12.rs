use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

use raw_window_handle::RawWindowHandle;
use smallvec::SmallVec;
use windows::{
    core::{Interface, PCSTR},
    w,
    Win32::{
        Foundation::{GetLastError, HANDLE, HWND},
        Graphics::{
            Direct3D::D3D_FEATURE_LEVEL_12_0,
            Direct3D12::{
                D3D12CreateDevice, D3D12GetDebugInterface, ID3D12CommandAllocator,
                ID3D12CommandList, ID3D12CommandQueue, ID3D12Debug, ID3D12DescriptorHeap,
                ID3D12Device, ID3D12Fence, ID3D12GraphicsCommandList, ID3D12InfoQueue1,
                ID3D12Resource, D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_QUEUE_DESC,
                D3D12_CPU_DESCRIPTOR_HANDLE, D3D12_DESCRIPTOR_HEAP_DESC,
                D3D12_DESCRIPTOR_HEAP_FLAG_NONE, D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                D3D12_FENCE_FLAG_NONE, D3D12_MESSAGE_CALLBACK_IGNORE_FILTERS,
                D3D12_MESSAGE_CATEGORY, D3D12_MESSAGE_ID, D3D12_MESSAGE_SEVERITY,
                D3D12_MESSAGE_SEVERITY_CORRUPTION, D3D12_MESSAGE_SEVERITY_ERROR,
                D3D12_MESSAGE_SEVERITY_INFO, D3D12_MESSAGE_SEVERITY_MESSAGE,
                D3D12_MESSAGE_SEVERITY_WARNING, D3D12_RESOURCE_BARRIER, D3D12_RESOURCE_BARRIER_0,
                D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES, D3D12_RESOURCE_BARRIER_FLAG_NONE,
                D3D12_RESOURCE_BARRIER_TYPE_TRANSITION, D3D12_RESOURCE_STATES,
                D3D12_RESOURCE_STATE_PRESENT, D3D12_RESOURCE_STATE_RENDER_TARGET,
                D3D12_RESOURCE_TRANSITION_BARRIER,
            },
            Dxgi::{
                Common::{
                    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT, DXGI_FORMAT_R16G16B16A16_FLOAT,
                    DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
                },
                CreateDXGIFactory2, DXGIGetDebugInterface1, IDXGIAdapter, IDXGIDebug1,
                IDXGIFactory6, IDXGISwapChain3, DXGI_CREATE_FACTORY_DEBUG, DXGI_DEBUG_ALL,
                DXGI_DEBUG_RLO_IGNORE_INTERNAL, DXGI_DEBUG_RLO_SUMMARY,
                DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE, DXGI_GPU_PREFERENCE_MINIMUM_POWER,
                DXGI_GPU_PREFERENCE_UNSPECIFIED, DXGI_MWA_NO_ALT_ENTER, DXGI_SCALING_NONE,
                DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
                DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT, DXGI_SWAP_EFFECT_FLIP_DISCARD,
                DXGI_USAGE_RENDER_TARGET_OUTPUT,
            },
        },
        System::Threading::{CreateEventW, WaitForSingleObject},
    },
};

use crate::{render_graph::RenderGraph, GraphicsConfig, PowerPreference};

pub struct GraphicsContext {
    dx: Rc<Interfaces>,
    graphics_queue: Rc<RefCell<GraphicsQueue>>,
}

impl GraphicsContext {
    pub fn new(config: &GraphicsConfig) -> Self {
        let dx = Interfaces::new(config);

        let graphics_queue = GraphicsQueue::new(&dx);

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

        let barriers = [
            transition_barrier(
                &target.resource,
                D3D12_RESOURCE_STATE_PRESENT,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
            ),
            transition_barrier(
                &target.resource,
                D3D12_RESOURCE_STATE_RENDER_TARGET,
                D3D12_RESOURCE_STATE_PRESENT,
            ),
        ];

        unsafe {
            command_list.ResourceBarrier(&barriers[..1]);
            command_list.ClearRenderTargetView(target.rtv, [0.5, 0.5, 0.5, 1.0].as_ptr(), &[]);
            command_list.ResourceBarrier(&barriers[1..]);
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

/// A `Surface` controls the acquisition and presentation of images to its
/// associated window.
pub struct Surface {
    // Use swapchain3 for color space support
    dx: Rc<Interfaces>,
    graphics_queue: Rc<RefCell<GraphicsQueue>>,
    flags: DXGI_SWAP_CHAIN_FLAG,
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

    fn new(dx: Rc<Interfaces>, queue: Rc<RefCell<GraphicsQueue>>, window: HWND) -> Self {
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
        dx: &Interfaces,
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

pub struct Image {
    resource: ID3D12Resource,
    last_use: Cell<u64>,
    rtv: D3D12_CPU_DESCRIPTOR_HANDLE,
}

struct Interfaces {
    is_debug: bool,
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

        if config.debug_mode {
            let queue: ID3D12InfoQueue1 = device.as_ref().unwrap().cast().unwrap();

            let mut cookie = 0;
            unsafe {
                queue.RegisterMessageCallback(
                    Some(Self::d3d12_debug_callback),
                    D3D12_MESSAGE_CALLBACK_IGNORE_FILTERS,
                    std::ptr::null(),
                    &mut cookie,
                )
            }
            .unwrap();
        }

        Self {
            is_debug: config.debug_mode,
            gi,
            device: device.unwrap(),
        }
    }

    extern "system" fn d3d12_debug_callback(
        _category: D3D12_MESSAGE_CATEGORY,
        severity: D3D12_MESSAGE_SEVERITY,
        id: D3D12_MESSAGE_ID,
        description: PCSTR,
        _context: *mut std::ffi::c_void,
    ) {
        println!(
            "D3D12: {}: {:?} {}",
            match severity {
                D3D12_MESSAGE_SEVERITY_CORRUPTION => "Corruption",
                D3D12_MESSAGE_SEVERITY_ERROR => "Error",
                D3D12_MESSAGE_SEVERITY_WARNING => "Warning",
                D3D12_MESSAGE_SEVERITY_INFO => "Info",
                D3D12_MESSAGE_SEVERITY_MESSAGE => "Message",
                _ => "Unknown severity",
            },
            id,
            unsafe { description.display() }
        );
    }
}

impl Drop for Interfaces {
    fn drop(&mut self) {
        if self.is_debug {
            let dxgi_debug: IDXGIDebug1 = unsafe { DXGIGetDebugInterface1(0) }.unwrap();
            unsafe {
                dxgi_debug.ReportLiveObjects(
                    DXGI_DEBUG_ALL,
                    DXGI_DEBUG_RLO_SUMMARY | DXGI_DEBUG_RLO_IGNORE_INTERNAL,
                )
            }
            .unwrap();
        }
    }
}

struct GraphicsQueue {
    queue: ID3D12CommandQueue,
    fence: ID3D12Fence,
    event: HANDLE,
    last_value: Cell<u64>,
    next_value: u64,
    allocators: Vec<ID3D12CommandAllocator>,
    command_lists: Vec<ID3D12GraphicsCommandList>,
    submissions: VecDeque<Submission>,
}

impl GraphicsQueue {
    fn new(dx: &Interfaces) -> Self {
        let queue: ID3D12CommandQueue = unsafe {
            dx.device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                ..Default::default()
            })
        }
        .unwrap();

        #[cfg(debug_assertions)]
        if dx.is_debug {
            unsafe { queue.SetName(w!("Graphics Queue")) }.unwrap();
        }

        let fence = unsafe { dx.device.CreateFence(0, D3D12_FENCE_FLAG_NONE) }.unwrap();
        let event = unsafe { CreateEventW(None, false, false, None) }.unwrap();

        assert_ne!(
            event,
            HANDLE(0),
            "CreateEventW failed. Error: {:?}",
            unsafe { GetLastError() }
        );

        let last_value = 0;
        let next_value = 1;

        unsafe { queue.Signal(&fence, next_value) }.unwrap();

        Self {
            queue,
            fence,
            event,
            last_value: Cell::new(last_value),
            next_value,
            command_lists: vec![],
            allocators: vec![],
            submissions: VecDeque::with_capacity(1),
        }
    }

    fn poll_fence(&self) -> u64 {
        self.last_value.set(
            self.last_value
                .get()
                .max(unsafe { self.fence.GetCompletedValue() }),
        );
        self.last_value.get()
    }

    fn is_complete(&self, fence_value: u64) -> bool {
        if fence_value > self.last_value.get() {
            self.poll_fence();
        }

        fence_value <= self.last_value.get()
    }

    fn flush(&mut self) {
        unsafe { self.queue.Signal(&self.fence, self.next_value).unwrap() };
        self.wait_until(self.next_value);
        self.next_value += 1;

        self.release_completed_submissions();
    }

    fn wait_until(&self, fence_value: u64) {
        if !self.is_complete(fence_value) {
            unsafe {
                self.fence
                    .SetEventOnCompletion(fence_value, self.event)
                    .unwrap();
                WaitForSingleObject(self.event, u32::MAX);
            }
            self.last_value.set(fence_value);
        }
    }

    fn release_completed_submissions(&mut self) {
        let mut i = 0;
        for submission in &self.submissions {
            if self.is_complete(submission.fence_value) {
                i += 1;
            } else {
                break;
            }
        }

        for mut submission in self.submissions.drain(..i) {
            for mut barrier in submission.barriers.drain(..) {
                if barrier.Type == D3D12_RESOURCE_BARRIER_TYPE_TRANSITION {
                    unsafe { std::mem::ManuallyDrop::drop(&mut barrier.Anonymous.Transition) };
                }
            }

            unsafe { submission.command_allocator.Reset() }.unwrap();
            self.allocators.push(submission.command_allocator);
        }
    }

    fn get_command_allocator(&mut self, dx: &Interfaces) -> ID3D12CommandAllocator {
        if let Some(allocator) = self.allocators.pop() {
            allocator
        } else {
            // Tentatively prefer lower memory usage over lower latency here.
            // Avoid creating a new allocator if we can at all avoid it. This is
            // just intuition; actual performance testing is needed here.
            let _ = self.poll_fence();
            self.release_completed_submissions();

            if let Some(allocator) = self.allocators.pop() {
                allocator
            } else {
                unsafe {
                    dx.device
                        .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
                }
                .unwrap()
            }
        }
    }

    fn get_command_list(
        &mut self,
        dx: &Interfaces,
        allocator: &ID3D12CommandAllocator,
    ) -> ID3D12GraphicsCommandList {
        if let Some(command_list) = self.command_lists.pop() {
            unsafe { command_list.Reset(allocator, None) }.unwrap();
            command_list
        } else {
            unsafe {
                dx.device
                    .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, allocator, None)
            }
            .unwrap()
        }
    }

    fn submit(
        &mut self,
        allocator: ID3D12CommandAllocator,
        commands: &[ID3D12GraphicsCommandList],
        barriers: SmallVec<[D3D12_RESOURCE_BARRIER; 2]>,
    ) -> u64 {
        unsafe {
            for command in commands {
                command.Close().unwrap();
            }

            // 4 is an arbitrary number
            let lists: SmallVec<[ID3D12CommandList; 4]> =
                commands.iter().map(|c| c.clone().cast().unwrap()).collect();

            self.queue.ExecuteCommandLists(&lists);
            self.queue.Signal(&self.fence, self.next_value).unwrap();
        }

        for command in commands {
            self.command_lists.push(command.clone());
        }

        self.submissions.push_back(Submission {
            fence_value: self.next_value,
            command_allocator: allocator,
            barriers,
        });

        let fence_value = self.next_value;
        self.next_value += 1;
        fence_value
    }
}

struct Submission {
    fence_value: u64,
    command_allocator: ID3D12CommandAllocator,
    barriers: SmallVec<[D3D12_RESOURCE_BARRIER; 2]>,
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
