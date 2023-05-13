use std::{cell::Cell, collections::VecDeque};

use smallvec::SmallVec;
#[allow(clippy::wildcard_imports)]
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

#[derive(Clone, Copy, Default)]
pub struct SubmissionId(u64);

pub struct Recording {
    pub commands: ID3D12GraphicsCommandList,
    pub barriers: SmallVec<[D3D12_RESOURCE_BARRIER; 2]>,
    pub allocator: ID3D12CommandAllocator,
}

struct Submission<T> {
    data: T,
    barriers: SmallVec<[D3D12_RESOURCE_BARRIER; 2]>,
    allocator: ID3D12CommandAllocator,
    fence_value: SubmissionId,
}

pub struct Graphics<T> {
    pub queue: ID3D12CommandQueue,
    fence: ID3D12Fence,
    event: HANDLE,
    last_value: Cell<u64>,
    next_value: Cell<u64>,
    commands: Vec<ID3D12GraphicsCommandList>,
    submissions: VecDeque<Submission<T>>,
}

impl<T> Graphics<T> {
    pub fn new(dx: &dx::Interfaces) -> Self {
        let queue: ID3D12CommandQueue = unsafe {
            dx.device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
                ..Default::default()
            })
        }
        .unwrap();

        let fence: ID3D12Fence =
            unsafe { dx.device.CreateFence(0, D3D12_FENCE_FLAG_NONE) }.unwrap();

        #[cfg(debug_assertions)]
        if dx.is_debug {
            unsafe {
                queue.SetName(w!("Graphics Queue")).unwrap();
                fence.SetName(w!("Graphics Fence")).unwrap();
            };
        }

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
            next_value: Cell::new(next_value),
            commands: Vec::new(),
            submissions: VecDeque::new(),
        }
    }

    pub fn poll_fence(&self) -> SubmissionId {
        self.last_value.set(
            self.last_value
                .get()
                .max(unsafe { self.fence.GetCompletedValue() }),
        );
        SubmissionId(self.last_value.get())
    }

    pub fn is_complete(&self, fence_value: SubmissionId) -> bool {
        if fence_value.0 > self.last_value.get() {
            self.poll_fence();
        }

        fence_value.0 <= self.last_value.get()
    }

    pub fn flush(&self) {
        let next_value = self.next_value.get();
        unsafe { self.queue.Signal(&self.fence, next_value).unwrap() };
        self.wait_until(SubmissionId(next_value));
        self.next_value.set(next_value + 1);
    }

    pub fn wait_until(&self, fence_value: SubmissionId) {
        if !self.is_complete(fence_value) {
            unsafe {
                self.fence
                    .SetEventOnCompletion(fence_value.0, self.event)
                    .unwrap();
                WaitForSingleObject(self.event, u32::MAX);
            }
            self.last_value.set(fence_value.0);
        }
    }

    pub fn record(&mut self, dx: &dx::Interfaces) -> (Recording, Option<T>) {
        let (allocator, barriers, value) = match self.submissions.front() {
            Some(submission) if self.is_complete(submission.fence_value) => {
                let mut submission = self.submissions.pop_front().unwrap();
                unsafe { submission.allocator.Reset().unwrap() };

                for mut barrier in submission.barriers.drain(..) {
                    match barrier.Type {
                        D3D12_RESOURCE_BARRIER_TYPE_TRANSITION => unsafe {
                            std::mem::ManuallyDrop::drop(&mut barrier.Anonymous.Transition);
                        },
                        _ => unimplemented!(),
                    }
                }

                (
                    submission.allocator,
                    submission.barriers,
                    Some(submission.data),
                )
            }
            _ => {
                let allocator: ID3D12CommandAllocator = unsafe {
                    dx.device
                        .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
                        .unwrap()
                };

                if dx.is_debug {
                    unsafe { allocator.SetName(w!("Graphics Command Allocator")).unwrap() };
                }

                (allocator, SmallVec::new(), None)
            }
        };

        // Note: naming command lists doesn't seem to do anything...?

        let commands = if let Some(commands) = self.commands.pop() {
            unsafe { commands.Reset(&allocator, None) }.unwrap();
            commands
        } else {
            unsafe {
                let command_list: ID3D12GraphicsCommandList = dx
                    .device
                    .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
                    .unwrap();

                command_list
            }
        };

        (
            Recording {
                commands,
                barriers,
                allocator,
            },
            value,
        )
    }

    pub fn submit(&mut self, recording: Recording, value: T) -> SubmissionId {
        let next_value = self.next_value.get();

        unsafe {
            recording.commands.Close().unwrap();
            let commands = recording.commands.cast().unwrap();
            self.queue.ExecuteCommandLists(&[commands]);
            self.queue.Signal(&self.fence, next_value).unwrap();
        }

        self.next_value.set(next_value + 1);
        let submission_id = SubmissionId(next_value);

        self.commands.push(recording.commands);
        self.submissions.push_back(Submission {
            data: value,
            barriers: recording.barriers,
            allocator: recording.allocator,
            fence_value: submission_id,
        });

        submission_id
    }
}
