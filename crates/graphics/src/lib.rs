//! Graphics!
//!
//! ## Goals
//!
//! - Feature set
//!  - Integrations with the platform windowing system for image presentation.
//!   - Note (2022-12-19): Should this instead belong to the shell?
//!  - Support for 2D rendering (and only 2D rendering).
//!   - Triangle Meshes
//!     - Vertex colors
//!     - Textured meshes
//!   - Vector graphics
//!     - SVG-compatible paths
//!   - Text
//!   - Images
//!   - Effects
//!    - Drop shadows
//!    - Blurs
//!    - Transparency & Color Filters
//!  - Render to texture
//!  - Render to window
//!
//! ## Thoughts & Rationale
//!
//! - Why not use a library like 'wgpu' instead of rolling your own graphics
//!   HAL?
//!  - 'wgpu' is currently in flux and is not yet stable. Furthermore, the
//!    anticipated feature set (see above) is simple enough that porting it to
//!    other platforms shouldn't be too difficult (I hope...).
//!
//! ## Development Timeline
//!
//!  A timeline of significant events in the development of this crate.
//!
//! - 2022-12-19: Work begins after a few false starts.

use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};

#[cfg(target_os = "windows")]
mod dx12;

#[cfg(target_os = "windows")]
use dx12 as platform;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum PowerPreference {
    #[default]
    DontCare,
    LowPower,
    HiPower,
}

/// Options for configuring the graphics context on initialization. Once set,
/// these options cannot be changed without recreating the graphics context.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsConfig {
    pub debug_mode: bool,
    pub power_preference: PowerPreference,
}

pub struct GraphicsContext {
    inner: platform::GraphicsContext,
}

impl GraphicsContext {
    pub fn new(config: &GraphicsConfig) -> Self {
        Self {
            inner: platform::GraphicsContext::new(config),
        }
    }

    pub fn create_surface(&self, window: impl HasRawWindowHandle) -> Surface {
        Surface {
            inner: self.inner.create_surface(window.raw_window_handle()),
        }
    }
}

pub struct Surface {
    inner: platform::Surface,
}

impl Surface {
    pub fn get_next_image(&mut self) -> SurfaceImage {
        SurfaceImage {
            inner: self.inner.get_next_image(),
        }
    }

    pub fn resize(&mut self) {
        self.inner.resize()
    }
}

pub struct SurfaceImage<'a> {
    inner: platform::SurfaceImage<'a>,
}

impl<'a> SurfaceImage<'a> {
    pub fn present(self) {
        self.inner.present()
    }
}
