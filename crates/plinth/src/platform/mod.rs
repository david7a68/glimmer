#[cfg(target_os = "windows")]
mod win32;

#[cfg(target_os = "windows")]
pub use win32::*;
