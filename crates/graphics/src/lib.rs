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

use std::cell::RefCell;

use geometry::{Extent, Point, Px, Rect};
use pixel_buffer::PixelBufferRef;
use raw_window_handle::HasRawWindowHandle;

mod memory;
mod pixel_buffer;
mod render_graph;

#[cfg(target_os = "windows")]
mod dx12;

#[cfg(target_os = "windows")]
use dx12 as platform;

pub use pixel_buffer::PixelBuffer;
pub use render_graph::{RenderGraph, RenderGraphNodeId};
use structures::generational_pool::{GenerationalPool, Handle};

#[derive(Clone, Copy, Debug)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const RED: Self = Self {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    pub const GREEN: Self = Self {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };

    pub const BLUE: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };

    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    #[must_use]
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoundedRectVertex {
    pub position: Point<f32, Px>,
    pub rect_size: Extent<f32, Px>,
    pub rect_center: Point<f32, Px>,
    pub outer_radii: [f32; 4],
    pub inner_radii: [f32; 4],
    pub color: Color,
}

pub enum RectPart<T> {
    Left(T),
    Right(T),
    Top(T),
    Bottom(T),
    TopLeft(T),
    TopRight(T),
    BottomLeft(T),
    BottomRight(T),
}

pub use RectPart::*;

#[derive(Clone)]
pub struct DrawRect {
    rect: Rect<f32, Px>,
    // top-left, top-right, bottom-left, bottom-right
    colors: [Color; 4],
    outer_radii: [f32; 4],
    inner_radii: [f32; 4],
    image: Option<(Image, [Point<f32, Px>; 4])>,
}

impl DrawRect {
    pub fn new(rect: Rect<f32, Px>) -> Self {
        Self {
            rect,
            colors: [Color::BLACK; 4],
            outer_radii: [0.0; 4],
            inner_radii: [0.0; 4],
            image: None,
        }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.colors = [color; 4];
        self
    }

    pub fn with_colors<const N: usize>(mut self, parts: [RectPart<Color>; N]) -> Self {
        for part in parts {
            match part {
                RectPart::Left(color) => {
                    self.colors[0] = color;
                    self.colors[3] = color;
                }
                RectPart::Right(color) => {
                    self.colors[1] = color;
                    self.colors[2] = color;
                }
                RectPart::Top(color) => {
                    self.colors[0] = color;
                    self.colors[1] = color;
                }
                RectPart::Bottom(color) => {
                    self.colors[2] = color;
                    self.colors[3] = color;
                }
                RectPart::TopLeft(color) => self.colors[0] = color,
                RectPart::TopRight(color) => self.colors[1] = color,
                RectPart::BottomLeft(color) => self.colors[2] = color,
                RectPart::BottomRight(color) => self.colors[3] = color,
            }
        }

        self
    }

    pub fn with_radius(mut self, radius: f32) -> Self {
        self.outer_radii = [radius; 4];
        self
    }

    pub fn with_radii<const N: usize>(mut self, parts: [RectPart<f32>; N]) -> Self {
        for part in parts {
            match part {
                RectPart::Left(radius) => {
                    self.outer_radii[2] = radius;
                    self.outer_radii[3] = radius;
                }
                RectPart::Right(radius) => {
                    self.outer_radii[0] = radius;
                    self.outer_radii[1] = radius;
                }
                RectPart::Top(radius) => {
                    self.outer_radii[1] = radius;
                    self.outer_radii[3] = radius;
                }
                RectPart::Bottom(radius) => {
                    self.outer_radii[0] = radius;
                    self.outer_radii[2] = radius;
                }
                RectPart::TopLeft(radius) => self.outer_radii[3] = radius,
                RectPart::TopRight(radius) => self.outer_radii[1] = radius,
                RectPart::BottomLeft(radius) => self.outer_radii[2] = radius,
                RectPart::BottomRight(radius) => self.outer_radii[0] = radius,
            }
        }

        self
    }

    pub(crate) fn to_vertices(&self) -> ([RoundedRectVertex; 4], [u16; 6]) {
        let Self {
            rect,
            colors,
            outer_radii,
            inner_radii,
            image: _,
        } = self;

        let rect_center = rect.center();

        let vertices = [
            RoundedRectVertex {
                position: rect.top_left(),
                rect_size: rect.extent(),
                rect_center,
                outer_radii: *outer_radii,
                inner_radii: *inner_radii,
                color: colors[0],
            },
            RoundedRectVertex {
                position: rect.top_right(),
                rect_size: rect.extent(),
                rect_center,
                outer_radii: *outer_radii,
                inner_radii: *inner_radii,
                color: colors[1],
            },
            RoundedRectVertex {
                position: rect.bottom_right(),
                rect_size: rect.extent(),
                rect_center,
                outer_radii: *outer_radii,
                inner_radii: *inner_radii,
                color: colors[2],
            },
            RoundedRectVertex {
                position: rect.bottom_left(),
                rect_size: rect.extent(),
                rect_center,
                outer_radii: *outer_radii,
                inner_radii: *inner_radii,
                color: colors[3],
            },
        ];

        let indices = [0, 1, 2, 0, 2, 3];

        (vertices, indices)
    }
}

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
    image_handles: RefCell<GenerationalPool<platform::Image>>,
    inner: RefCell<platform::GraphicsContext>,
}

impl GraphicsContext {
    #[must_use]
    pub fn new(config: &GraphicsConfig) -> Self {
        Self {
            image_handles: GenerationalPool::new().into(),
            inner: RefCell::new(platform::GraphicsContext::new(config)),
        }
    }

    #[must_use]
    pub fn create_surface(&self, window: impl HasRawWindowHandle) -> Surface {
        Surface {
            inner: self
                .inner
                .borrow()
                .create_surface(window.raw_window_handle()),
        }
    }

    pub fn destroy_surface(&self, surface: &mut Surface) {
        self.inner.borrow().destroy_surface(&mut surface.inner);
    }

    pub fn get_next_image<'a>(&self, surface: &'a mut Surface) -> RenderTarget<'a> {
        let inner = self.inner.borrow().get_next_image(&mut surface.inner);
        RenderTarget { inner }
    }

    pub fn present(&self, surface: &mut Surface) {
        self.inner.borrow().present(&mut surface.inner);
    }

    pub fn resize(&self, surface: &mut Surface) {
        self.inner.borrow().resize(&mut surface.inner);
    }

    pub fn draw(&self, target: &RenderTarget, content: &RenderGraph) {
        self.inner.borrow_mut().draw(&target.inner, content);
    }

    pub fn upload_image(&self, pixels: PixelBufferRef) -> Image {
        let image = self.inner.borrow_mut().upload_image(pixels);
        let handle = self.image_handles.borrow_mut().insert(image);
        Image { handle }
    }

    pub fn destroy_image(&self, image: &mut Image) {
        let image = self.image_handles.borrow_mut().remove(image.handle);
        std::mem::drop(image);
    }
}

pub struct Surface {
    inner: platform::Surface,
}

pub struct RenderTarget<'a> {
    inner: platform::RenderTarget<'a>,
}

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Image {
    handle: Handle<platform::Image>,
}
