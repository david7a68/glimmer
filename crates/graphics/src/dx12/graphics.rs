use std::{cell::Cell, collections::VecDeque};

use smallvec::SmallVec;
use windows::{
    core::Interface,
    w,
    Win32::{
        Foundation::{GetLastError, HANDLE},
        Graphics::Direct3D12::*,
        System::Threading::{CreateEventW, WaitForSingleObject},
    },
};

use super::dx;

pub struct Queue {
    pub queue: ID3D12CommandQueue,
    fence: ID3D12Fence,
    event: HANDLE,
    last_value: Cell<u64>,
    next_value: u64,
    allocators: Vec<ID3D12CommandAllocator>,
    command_lists: Vec<ID3D12GraphicsCommandList>,
    submissions: VecDeque<Submission>,
}

impl Queue {
    pub fn new(dx: &dx::Interfaces) -> Self {
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

    pub fn poll_fence(&self) -> u64 {
        self.last_value.set(
            self.last_value
                .get()
                .max(unsafe { self.fence.GetCompletedValue() }),
        );
        self.last_value.get()
    }

    pub fn is_complete(&self, fence_value: u64) -> bool {
        if fence_value > self.last_value.get() {
            self.poll_fence();
        }

        fence_value <= self.last_value.get()
    }

    pub fn flush(&mut self) {
        unsafe { self.queue.Signal(&self.fence, self.next_value).unwrap() };
        self.wait_until(self.next_value);
        self.next_value += 1;

        self.release_completed_submissions();
    }

    pub fn wait_until(&self, fence_value: u64) {
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

    pub fn release_completed_submissions(&mut self) {
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

    pub fn get_command_allocator(&mut self, dx: &dx::Interfaces) -> ID3D12CommandAllocator {
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

    pub fn get_command_list(
        &mut self,
        dx: &dx::Interfaces,
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

    pub fn submit(
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
