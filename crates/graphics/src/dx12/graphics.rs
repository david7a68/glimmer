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

    pub fn submit(&mut self, commands: &ID3D12GraphicsCommandList) -> u64 {
        unsafe {
            commands.Close().unwrap();
            let commands = commands.cast().unwrap();
            self.queue.ExecuteCommandLists(&[commands]);
            self.queue.Signal(&self.fence, self.next_value).unwrap();
        }

        let fence_value = self.next_value;
        self.next_value += 1;
        fence_value
    }
}
