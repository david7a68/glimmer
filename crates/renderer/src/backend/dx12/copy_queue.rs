use std::{ffi::c_void, mem::ManuallyDrop, sync::Arc};

use geometry::{Point, Px, Rect};
use parking_lot::RwLock;
use smallvec::SmallVec;
use structures::generational_pool::GenerationalPool;
use windows::{
    core::ComInterface,
    Win32::{
        Foundation::{GetLastError, HANDLE},
        Graphics::{
            Direct3D12::{
                ID3D12CommandAllocator, ID3D12CommandQueue, ID3D12Device, ID3D12Fence,
                ID3D12GraphicsCommandList, ID3D12Resource, D3D12_BOX, D3D12_COMMAND_LIST_TYPE_COPY,
                D3D12_COMMAND_QUEUE_DESC, D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
                D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                D3D12_SUBRESOURCE_FOOTPRINT, D3D12_TEXTURE_COPY_LOCATION,
                D3D12_TEXTURE_COPY_LOCATION_0, D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX, D3D12_TEXTURE_DATA_PITCH_ALIGNMENT,
                D3D12_TEXTURE_DATA_PLACEMENT_ALIGNMENT,
            },
            Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
        },
        System::Threading::{CreateEventW, WaitForSingleObject},
    },
};

use crate::{
    backend::{linear_allocator::LinearAllocator, next_multiple_of},
    image::{PixelBuffer, PixelFormat},
};

use super::{transition_barrier, Image, ImageData};

pub struct BufferResource {
    pub ptr: *mut c_void,
    pub size: u64,
    pub offset: u64,
    pub resource: ID3D12Resource,
}

struct ImageCopy {
    src: D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
    in_region: Rect<u32, Px>,
    to: Image,
    at: Point<u32, Px>,
}

pub struct CopyQueue {
    flushing: bool,

    event: HANDLE,
    fence: ID3D12Fence,
    fence_value: u64,

    queue: ID3D12CommandQueue,
    images: Vec<ImageCopy>,
    image_pool: Arc<RwLock<GenerationalPool<RwLock<ImageData>>>>,

    buffer_offset: u64,
    buffer_resource: ID3D12Resource,
    buffer_allocator: LinearAllocator,

    command_list: ID3D12GraphicsCommandList,
    command_allocator: ID3D12CommandAllocator,
}

impl CopyQueue {
    pub(super) fn new(
        device: &ID3D12Device,
        image_pool: Arc<RwLock<GenerationalPool<RwLock<ImageData>>>>,
        fence: ID3D12Fence,
        staging_buffer: BufferResource,
    ) -> Self {
        let queue: ID3D12CommandQueue = unsafe {
            device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                Type: D3D12_COMMAND_LIST_TYPE_COPY,
                ..Default::default()
            })
        }
        .unwrap();

        let event = unsafe { CreateEventW(None, false, false, None) }.unwrap();

        assert_ne!(
            event,
            HANDLE(0),
            "CreateEventW failed. Error: {:?}",
            unsafe { GetLastError() }
        );

        unsafe { queue.Signal(&fence, 0) }.unwrap();

        let command_allocator =
            unsafe { device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_COPY) }.unwrap();

        let command_list = unsafe {
            device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_COPY, &command_allocator, None)
        }
        .unwrap();

        Self {
            flushing: false,
            event,
            fence,
            fence_value: 0,
            images: Vec::new(),
            image_pool,
            queue,
            buffer_offset: staging_buffer.offset,
            buffer_resource: staging_buffer.resource,
            buffer_allocator: LinearAllocator::new(staging_buffer.size, staging_buffer.ptr),
            command_list,
            command_allocator,
        }
    }

    pub fn flush(&mut self) {
        let image_pool = self.image_pool.read();

        let mut images = {
            let mut handles = self
                .images
                .drain(..)
                .map(|copy| (copy.to.0, copy))
                .collect::<SmallVec<[_; 64]>>();
            handles.sort_unstable_by_key(|(handle, _)| *handle);
            handles.dedup_by_key(|(handle, _)| *handle);

            handles
                .into_iter()
                .map(|(handle, copy)| (copy, image_pool.get(handle).unwrap().write()))
                .collect::<SmallVec<[_; 64]>>()
        };

        let mut read_fence_value = 0;

        for (copy, lock) in &images {
            transition_barrier(
                &lock.resource,
                D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                D3D12_RESOURCE_STATE_COPY_DEST,
            );

            if lock.read_id > read_fence_value {
                read_fence_value = lock.read_id;
            }

            let dst_location = D3D12_TEXTURE_COPY_LOCATION {
                pResource: ManuallyDrop::new(Some(unsafe {
                    std::mem::transmute_copy(&lock.resource)
                })),
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    SubresourceIndex: 0,
                },
            };

            let src_location = D3D12_TEXTURE_COPY_LOCATION {
                pResource: ManuallyDrop::new(Some(unsafe {
                    std::mem::transmute_copy(&self.buffer_resource)
                })),
                Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                    PlacedFootprint: copy.src,
                },
            };

            let src_box = D3D12_BOX {
                left: copy.in_region.left(),
                top: copy.in_region.top(),
                right: copy.in_region.right(),
                bottom: copy.in_region.bottom(),
                front: 0,
                back: 1,
            };

            unsafe {
                self.command_list.CopyTextureRegion(
                    &dst_location,
                    copy.at.x,
                    copy.at.y,
                    0,
                    &src_location,
                    Some(&src_box),
                )
            }

            transition_barrier(
                &lock.resource,
                D3D12_RESOURCE_STATE_COPY_DEST,
                D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
            );
        }

        unsafe {
            self.queue.Wait(&self.fence, read_fence_value).unwrap();

            self.command_list.Close().unwrap();
            let list = self.command_list.cast().unwrap();
            self.queue.ExecuteCommandLists(&[Some(list)]);

            self.fence_value += 1;
            self.queue.Signal(&self.fence, self.fence_value).unwrap();
        }

        for (_, mut lock) in images.drain(..) {
            lock.write_id = self.fence_value;
        }

        self.flushing = true;
    }

    pub fn copy_pixels(
        &mut self,
        src: PixelBuffer,
        in_region: Rect<u32, Px>,
        dst: &Image,
        at: Point<u32, Px>,
    ) {
        if self.flushing {
            unsafe {
                self.fence.SetEventOnCompletion(1, self.event).unwrap();
                WaitForSingleObject(self.event, u32::MAX);
            }

            self.flushing = false;
            self.buffer_allocator.clear();
            unsafe { self.command_allocator.Reset() }.unwrap();
        }

        let row_pitch = next_multiple_of(
            src.width() as u64 * src.format().bytes_per_pixel() as u64,
            D3D12_TEXTURE_DATA_PITCH_ALIGNMENT as u64,
        );

        let buffer_size = row_pitch * src.height() as u64;
        assert!(buffer_size <= self.buffer_allocator.capacity());

        if !self
            .buffer_allocator
            .can_fit(buffer_size, D3D12_TEXTURE_DATA_PLACEMENT_ALIGNMENT as u64)
        {
            self.flush();
        }

        let (buffer_offset, buffer) = self
            .buffer_allocator
            .allocate(buffer_size, D3D12_TEXTURE_DATA_PLACEMENT_ALIGNMENT as u64)
            .unwrap();

        for (i, row) in src.rows().enumerate() {
            let offset = i as u64 * row_pitch;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    row.as_ptr(),
                    buffer.as_mut_ptr().add(offset as usize),
                    row.len(),
                );
            }

            // do we need to zero th rest of the row?
        }

        let format = match src.format() {
            PixelFormat::RgbaU8 => DXGI_FORMAT_R8G8B8A8_UNORM,
        };

        let placed_desc = D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
            Offset: buffer_offset + self.buffer_offset,
            Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                Format: format,
                Width: src.width(),
                Height: src.height(),
                Depth: 1,
                RowPitch: row_pitch as u32,
            },
        };

        self.images.push(ImageCopy {
            src: placed_desc,
            in_region,
            to: dst.clone(),
            at,
        });
    }
}
