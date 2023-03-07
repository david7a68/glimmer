use windows::{
    core::{Interface, PCSTR},
    Win32::Graphics::{Direct3D::D3D_FEATURE_LEVEL_11_0, Direct3D12::*, Dxgi::*},
};

use crate::{GraphicsConfig, PowerPreference};

pub struct Interfaces {
    pub is_debug: bool,
    pub gi: IDXGIFactory6,
    pub device: ID3D12Device,
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
        unsafe { D3D12CreateDevice(&adapter, D3D_FEATURE_LEVEL_11_0, &mut device) }.unwrap();

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
