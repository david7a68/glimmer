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
//!   - Rounded Rectangles
//!     - Inner and outer radii
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

pub mod graphics;
pub mod input;
pub mod window;

mod memory;
mod platform;

use graphics::GraphicsConfig;
pub use graphics::PowerPreference;

pub struct Config {
    pub debug_mode: bool,
    pub power_preference: PowerPreference,
}

pub struct Plinth {
    platform: platform::Platform,
}

impl Plinth {
    #[must_use]
    pub fn new(config: &Config) -> Self {
        Self {
            platform: platform::Platform::new(&GraphicsConfig {
                debug_mode: config.debug_mode,
                power_preference: config.power_preference,
            }),
        }
    }

    pub fn create_window(&mut self) -> () {
        todo!()
    }

    // run_event_loop()
}
