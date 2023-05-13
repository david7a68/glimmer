#[cfg(target_os = "windows")]
mod dx12;

#[cfg(target_os = "windows")]
pub use dx12::*;

mod linear_allocator;
mod ring_allocator;

pub(crate) fn next_multiple_of(a: u64, b: u64) -> u64 {
    match a % b {
        0 => a,
        r => a + b - r,
    }
}
