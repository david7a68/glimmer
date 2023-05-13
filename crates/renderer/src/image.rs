use crate::{backend, Renderer};
use geometry::{Extent, Px};

#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Direct3D12::ID3D12Resource;

#[derive(Clone, Copy, Debug)]
pub enum PixelFormat {
    RgbaU8,
}

impl PixelFormat {
    pub fn bytes_per_pixel(&self) -> u32 {
        match self {
            PixelFormat::RgbaU8 => 4,
        }
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub enum ColorSpace {
    #[default]
    Srgb,
}

/// An image resource.
pub struct Image {
    image: backend::Image,
}

impl Image {
    pub fn new(
        renderer: &mut Renderer,
        size: Extent<u32, Px>,
        format: PixelFormat,
        color_space: Option<ColorSpace>,
    ) -> Self {
        Self {
            image: renderer.backend.create_image(
                size,
                format,
                color_space.unwrap_or(ColorSpace::Srgb),
            ),
        }
    }

    pub fn from_buffer(renderer: &mut Renderer, pixel_buffer: PixelBuffer) -> Self {
        let image = Self {
            image: renderer.backend.create_image(
                pixel_buffer.size,
                pixel_buffer.format,
                pixel_buffer.color_space,
            ),
        };

        // renderer.copy(image, bytes, dst, src);

        image
    }

    #[cfg(target_os = "windows")]
    pub fn from_dx12_resource(
        renderer: &mut Renderer,
        resource: &ID3D12Resource,
        color_space: ColorSpace,
    ) -> Self {
        Self {
            image: renderer
                .backend
                .create_external_image(resource.clone(), color_space),
        }
    }

    pub fn format(&self) -> PixelFormat {
        PixelFormat::RgbaU8
    }

    pub fn extent(&self) -> Extent<u32, Px> {
        Extent::new(0, 0)
    }

    /// Returns the image data as a byte slice.
    ///
    /// This may block if the image is not in a CPU-accessible memory location.
    pub fn get_pixels(&self) -> PixelBuffer {
        todo!()
    }
}

pub struct PixelBuffer {
    bytes: Box<[u8]>,
    size: Extent<u32, Px>,
    format: PixelFormat,
    color_space: ColorSpace,
}

impl PixelBuffer {
    pub fn new(
        bytes: Box<[u8]>,
        size: Extent<u32, Px>,
        format: PixelFormat,
        color_space: ColorSpace,
    ) -> Self {
        assert!(
            bytes.len() == (size.width * size.height) as usize * format.bytes_per_pixel() as usize
        );

        Self {
            bytes,
            size,
            format,
            color_space,
        }
    }

    pub fn width(&self) -> u32 {
        self.size.width
    }

    pub fn height(&self) -> u32 {
        self.size.height
    }

    pub fn format(&self) -> PixelFormat {
        self.format
    }

    pub fn color_space(&self) -> ColorSpace {
        self.color_space
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn rows(&self) -> RowIter {
        let row_pitch = self.size.width as usize * self.format.bytes_per_pixel() as usize;

        RowIter {
            row_pitch,
            cursor: 0,
            bytes: self.bytes.as_ref(),
        }
    }
}

pub struct RowIter<'a> {
    row_pitch: usize,
    cursor: usize,
    bytes: &'a [u8],
}

impl<'a> Iterator for RowIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.bytes.len() {
            return None;
        }

        let row = &self.bytes[self.cursor..self.cursor + self.row_pitch];
        self.cursor += self.row_pitch;
        Some(row)
    }
}
