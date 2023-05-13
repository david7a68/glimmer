// mod copy_queue;

use std::{mem::ManuallyDrop, sync::Arc};

use parking_lot::RwLock;
use windows::Win32::Graphics::{
    Direct3D::D3D_FEATURE_LEVEL_11_0,
    Direct3D12::{
        D3D12CreateDevice, D3D12GetDebugInterface, ID3D12Debug, ID3D12Device, ID3D12Resource,
        D3D12_CPU_PAGE_PROPERTY_UNKNOWN, D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT,
        D3D12_HEAP_FLAG_NONE, D3D12_HEAP_PROPERTIES, D3D12_HEAP_TYPE_DEFAULT,
        D3D12_HEAP_TYPE_UPLOAD, D3D12_MEMORY_POOL_UNKNOWN, D3D12_RESOURCE_BARRIER,
        D3D12_RESOURCE_BARRIER_0, D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
        D3D12_RESOURCE_BARRIER_FLAG_NONE, D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        D3D12_RESOURCE_DESC, D3D12_RESOURCE_DIMENSION_BUFFER, D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        D3D12_RESOURCE_FLAG_NONE, D3D12_RESOURCE_STATES, D3D12_RESOURCE_STATE_COPY_DEST,
        D3D12_RESOURCE_STATE_GENERIC_READ, D3D12_RESOURCE_TRANSITION_BARRIER,
        D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
    },
    Dxgi::{
        Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC},
        CreateDXGIFactory2, IDXGIAdapter, IDXGIFactory6, DXGI_CREATE_FACTORY_DEBUG,
        DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE, DXGI_GPU_PREFERENCE_MINIMUM_POWER,
    },
};

use geometry::{Extent, Point, Px, Rect};
use structures::generational_pool::{GenerationalPool, Handle};

use crate::{
    image::{ColorSpace, PixelBuffer, PixelFormat},
    Config, PowerPreference,
};

use super::{next_multiple_of, ring_allocator::RingAllocator};

// use self::copy_queue::{BufferResource, CopyQueue};

#[derive(Clone, Copy, Debug)]
pub struct Image(Handle<RwLock<ImageData>>);

pub struct ImageData {
    is_owned: bool,
    color_space: ColorSpace,
    resource: ID3D12Resource,
    read_id: u64,
    write_id: u64,
}

pub struct Backend {
    device: ID3D12Device,

    dxgi_factory: IDXGIFactory6,

    cpu_buffer: ID3D12Resource,

    image_pool: Arc<RwLock<GenerationalPool<RwLock<ImageData>>>>,

    // copy_queue_proxy: CopyQueueProxy,
    /// Allocator used for uploading small amounts of dynamic data to the GPU.
    per_draw_allocator: RingAllocator,
}

impl Backend {
    pub fn new(config: &Config) -> Self {
        let enable_debug = if let Some(debug) = config.debug_mode {
            debug
        } else if cfg!(debug_assertions) {
            true
        } else {
            false
        };

        let dxgi_factory: IDXGIFactory6 = {
            let flags = if enable_debug {
                DXGI_CREATE_FACTORY_DEBUG
            } else {
                0
            };

            unsafe { CreateDXGIFactory2(flags) }.unwrap()
        };

        let power_preference = match config.power_preference {
            PowerPreference::LowPower => DXGI_GPU_PREFERENCE_MINIMUM_POWER,
            PowerPreference::HighPerformance => DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
        };

        let adapter: IDXGIAdapter =
            unsafe { dxgi_factory.EnumAdapterByGpuPreference(0, power_preference) }
                .or_else(|_| unsafe { dxgi_factory.EnumWarpAdapter() })
                .unwrap();

        if enable_debug {
            let mut dx_debug: Option<ID3D12Debug> = None;
            unsafe { D3D12GetDebugInterface(&mut dx_debug) }.unwrap();
            unsafe { dx_debug.unwrap().EnableDebugLayer() };
        }

        let device = {
            let mut device: Option<ID3D12Device> = None;
            unsafe { D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device) }.unwrap();
            device.unwrap()
        };

        let dynamic_upload_buffer_size = next_multiple_of(
            config.dynamic_upload_buffer_size,
            D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT as u64,
        );

        let staging_buffer_size = next_multiple_of(
            config.staging_buffer_size,
            D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT as u64,
        );

        let cpu_buffer: ID3D12Resource = {
            let mut buffer = None;
            unsafe {
                device.CreateCommittedResource(
                    &D3D12_HEAP_PROPERTIES {
                        Type: D3D12_HEAP_TYPE_UPLOAD,
                        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
                        MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
                        CreationNodeMask: 0,
                        VisibleNodeMask: 0,
                    },
                    D3D12_HEAP_FLAG_NONE, // set automatically by CreateCommitedResource
                    &D3D12_RESOURCE_DESC {
                        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                        Alignment: 0, // default: 64k
                        Width: dynamic_upload_buffer_size + staging_buffer_size,
                        Height: 1,
                        DepthOrArraySize: 1,
                        MipLevels: 1,
                        Format: DXGI_FORMAT_UNKNOWN,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                        Flags: D3D12_RESOURCE_FLAG_NONE,
                    },
                    D3D12_RESOURCE_STATE_GENERIC_READ,
                    None,
                    &mut buffer,
                )
            }
            .unwrap();
            buffer.unwrap()
        };

        let cpu_buffer_ptr = {
            let mut ptr = std::ptr::null_mut();
            unsafe { cpu_buffer.Map(0, None, Some(&mut ptr)) }.unwrap();
            ptr.cast()
        };

        let per_draw_allocator = RingAllocator::new(dynamic_upload_buffer_size, cpu_buffer_ptr);

        Self {
            device,
            dxgi_factory,
            cpu_buffer,
            image_pool: Arc::new(RwLock::new(GenerationalPool::new())),
            per_draw_allocator,
        }
    }

    pub fn create_image(
        &mut self,
        size: Extent<u32, Px>,
        format: PixelFormat,
        color_space: ColorSpace,
    ) -> Image {
        let format = match format {
            PixelFormat::RgbaU8 => DXGI_FORMAT_R8G8B8A8_UNORM,
        };

        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Alignment: 0,
            Width: size.width as u64,
            Height: size.height,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: D3D12_RESOURCE_FLAG_NONE,
        };

        let mut resource: Option<ID3D12Resource> = None;
        unsafe {
            self.device
                .CreateCommittedResource(
                    &D3D12_HEAP_PROPERTIES {
                        Type: D3D12_HEAP_TYPE_DEFAULT,
                        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
                        MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
                        CreationNodeMask: 0,
                        VisibleNodeMask: 0,
                    },
                    D3D12_HEAP_FLAG_NONE,
                    &desc,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    None,
                    &mut resource,
                )
                .unwrap();
        }

        let resource = resource.unwrap();

        Image(self.image_pool.write().insert(RwLock::new(ImageData {
            is_owned: true,
            color_space,
            resource,
            read_id: 0,
            write_id: 0,
        })))
    }

    pub fn create_external_image(
        &mut self,
        resource: ID3D12Resource,
        color_space: ColorSpace,
    ) -> Image {
        Image(self.image_pool.write().insert(RwLock::new(ImageData {
            is_owned: false,
            color_space,
            resource,
            read_id: 0,
            write_id: 0,
        })))
    }
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
            Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: ManuallyDrop::new(Some(unsafe { std::mem::transmute_copy(&resource) })),
                StateBefore: state_before,
                StateAfter: state_after,
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
            }),
        },
    }
}
