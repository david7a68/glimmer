//! 2D gui renderer
//!
//! ## Goals
//!
//! - [ ] GPU accelerated rendering
//! - [ ] Fixed memory usage
//! - [ ] Predictable performance
//!
//! ## Non-goals
//!
//! - [ ] Compositor integration (drawing directly to windows)

pub mod canvas;
pub mod color;
pub mod image;
pub mod rect;

mod backend;

#[derive(Clone, Copy, Debug)]
pub enum PowerPreference {
    LowPower,
    HighPerformance,
}

#[derive(Debug)]
pub struct Config {
    /// The power preference for the renderer.
    ///
    /// This influences the GPU selection criteria in multi-gpu systems. Setting
    /// `LowPower` mode prefers integrated GPUs over discrete GPUs, and setting
    /// `HighPerformance` does the reverse. Defaults to `LowPower`.
    pub power_preference: PowerPreference,

    /// Whether or not to enable debugging features.
    ///
    /// This may have an outsized impact on performance. Defaults to `None`,
    /// which automatically enables debugging features in debug builds. Override
    /// with `Some(true)` or `Some(false)` to force enable or disable debugging.
    pub debug_mode: Option<bool>,

    /// The amount of GPU memory to allocate for data that is updated per draw.
    ///
    /// Defaults to 1 Mib.
    pub dynamic_upload_buffer_size: u64,

    /// The amount of memory to allocate for staging images for upload.
    ///
    /// Defaults to 4 Mib.
    pub staging_buffer_size: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            power_preference: PowerPreference::LowPower,
            debug_mode: None,
            dynamic_upload_buffer_size: 1 * 1024 * 1024,
            staging_buffer_size: 4 * 1024 * 1024,
        }
    }
}

/// Shared renderer state.
pub struct Renderer {
    backend: backend::Backend,
    // copy_queue: backend::CopyQueue,
}

impl Renderer {
    pub fn new(config: &Config) -> Self {
        let backend = backend::Backend::new(config);
        // let copy_queue = backend.init_copy_queue();

        Self {
            backend: backend::Backend::new(config),
        }
    }
}
