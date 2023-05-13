use geometry::{Extent, Px};

use crate::{
    image::{ColorSpace, Image, PixelFormat},
    Renderer,
};

/// A render target.
pub struct Canvas {}

impl Canvas {
    pub fn new(
        renderer: &Renderer,
        size: Extent<u32, Px>,
        format: PixelFormat,
        color_space: Option<ColorSpace>,
    ) -> Self {
        Self {}
    }

    pub fn from_image(renderer: &Renderer, image: &Image) -> Self {
        Self {}
    }

    // draw()

    // finish() -> Image
}
