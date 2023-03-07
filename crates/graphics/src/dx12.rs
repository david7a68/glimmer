use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

use raw_window_handle::RawWindowHandle;
use smallvec::SmallVec;
use windows::{
    core::Interface,
    Win32::{
        Foundation::{GetLastError, HANDLE, HWND},
        Graphics::{
            Direct3D::D3D_FEATURE_LEVEL_12_0,
            Direct3D12::{
                D3D12CreateDevice, D3D12GetDebugInterface, ID3D12CommandAllocator,
                ID3D12CommandList, ID3D12CommandQueue, ID3D12Debug, ID3D12DescriptorHeap,
                ID3D12Device, ID3D12Fence, ID3D12GraphicsCommandList, ID3D12Resource,
                D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_COMMAND_QUEUE_DESC,
                D3D12_CPU_DESCRIPTOR_HANDLE, D3D12_DESCRIPTOR_HEAP_DESC,
                D3D12_DESCRIPTOR_HEAP_FLAG_NONE, D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                D3D12_FENCE_FLAG_NONE, D3D12_RESOURCE_BARRIER, D3D12_RESOURCE_BARRIER_0,
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

    pub fn draw(&mut self, target: &Image, content: &RenderGraph) {
        let mut graphics = self.graphics_queue.borrow_mut();

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

        let fence_value = graphics.submit(allocator, &[command_list]);
        target.last_use.set(fence_value);
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
    dx: Rc<Interfaces>,
    graphics_queue: Rc<RefCell<GraphicsQueue>>,
    flags: DXGI_SWAP_CHAIN_FLAG,
    swapchain: IDXGISwapChain3,
    image_index: u32,
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
            render_targets: [Some(a), Some(b)],
            waitable_object,
            rtv_heap,
        }
    }

    pub fn resize(&mut self) {
        // make sure that the render targets aren't currently in use
        let graphics_queue = self.graphics_queue.borrow();
        graphics_queue.wait_until(self.render_targets[0].as_ref().unwrap().last_use.get());
        graphics_queue.wait_until(self.render_targets[1].as_ref().unwrap().last_use.get());

        // need to reset command allocators?

        // destroy the old render targets
        {
            let _ = self.render_targets[0].take();
            let _ = self.render_targets[1].take();
        }
        // self.rtv_heap = unsafe {
        //     self.dx
        //         .device
        //         .CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
        //             Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
        //             NumDescriptors: Self::BUFFER_COUNT,
        //             Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
        //             NodeMask: 0,
        //         })
        // }
        // .unwrap();

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
        unsafe { WaitForSingleObjectEx(self.waitable_object, u32::MAX, false) };

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

            let buffer0 = swapchain.GetBuffer(0).unwrap();
            let rtv0 = heap_start;
            dx.device.CreateRenderTargetView(&buffer0, None, heap_start);

            let buffer1 = swapchain.GetBuffer(1).unwrap();
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

    pub fn get_image(&self) -> &Image {
        self.surface.render_targets[self.surface.image_index as usize]
            .as_ref()
            .unwrap()
    }
}

#[derive(Clone)]
pub struct Image {
    resource: ID3D12Resource,
    last_use: Cell<u64>,
    rtv: D3D12_CPU_DESCRIPTOR_HANDLE,
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

struct GraphicsQueue {
    queue: ID3D12CommandQueue,
    fence: ID3D12Fence,
    event: HANDLE,
    last_value: Cell<u64>,
    next_value: u64,
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

    fn wait_until(&self, fence_value: u64) {
        if !self.is_complete(fence_value) {
            unsafe {
                self.fence
                    .SetEventOnCompletion(fence_value, self.event)
                    .unwrap();
                WaitForSingleObjectEx(self.event, u32::MAX, false);
            }
        }
    }

    fn wait_idle(&self) {
        unsafe {
            self.fence
                .SetEventOnCompletion(self.next_value - 1, self.event)
                .unwrap();
            WaitForSingleObjectEx(self.event, u32::MAX, false);
        }
    }

    fn get_command_allocator(&mut self, dx: &Interfaces) -> ID3D12CommandAllocator {
        let allocator = if let Some(submission) = self.submissions.pop_front() {
            self.is_complete(submission.fence_value);
            submission.command_allocator
        } else {
            unsafe {
                dx.device
                    .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
            }
            .unwrap()
        };

        unsafe { allocator.Reset() }.unwrap();
        allocator
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
    ) -> u64 {
        unsafe {
            for command in commands {
                command.Close().unwrap();
            }

            // 4 is an arbitrary number
            let lists: SmallVec<[Option<ID3D12CommandList>; 4]> = commands
                .iter()
                .map(|c| Some(c.clone().cast().unwrap()))
                .collect();

            self.queue.ExecuteCommandLists(&lists);
            self.queue.Signal(&self.fence, self.next_value).unwrap();
        }

        for command in commands {
            self.command_lists.push(command.clone());
        }

        self.submissions.push_back(Submission {
            fence_value: self.next_value,
            command_allocator: allocator,
        });

        let fence_value = self.next_value;
        self.next_value += 1;
        fence_value
    }
}

struct Submission {
    fence_value: u64,
    command_allocator: ID3D12CommandAllocator,
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
                pResource: Some(resource.clone()),
                StateBefore: state_before,
                StateAfter: state_after,
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
            }),
        },
    }
}
