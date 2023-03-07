use std::ops::Deref;

use crate::Color;

/// Describes the binary representation of a pixel in a pixel buffer.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Rgba8,
}

impl PixelFormat {
    const MAX_BYTES_PER_PIXEL: usize = std::mem::size_of::<f32>() * 4;

    #[must_use]
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgba8 => 4,
        }
    }

    pub fn read_color(self, bytes: &[u8]) -> Color {
        match self {
            Self::Rgba8 => {
                let r = bytes[0] as f32 / 255.0;
                let g = bytes[1] as f32 / 255.0;
                let b = bytes[2] as f32 / 255.0;
                let a = bytes[3] as f32 / 255.0;
                Color::new(r, g, b, a)
            }
        }
    }

    pub fn write_color(self, color: Color) -> (u8, [u8; Self::MAX_BYTES_PER_PIXEL]) {
        match self {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Self::Rgba8 => {
                let bytes = [
                    (color.r.clamp(0.0, 1.0) * 255.0).round() as u8,
                    (color.g.clamp(0.0, 1.0) * 255.0).round() as u8,
                    (color.b.clamp(0.0, 1.0) * 255.0).round() as u8,
                    (color.a.clamp(0.0, 1.0) * 255.0).round() as u8,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                ];

                (4, bytes)
            }
        }
    }
}

/// Describes how the color values of a pixel buffer are interpreted.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSpace {
    Srgb,
}

/// A reference-counted pixel buffer.
#[derive(Clone)]
pub struct PixelBuffer {
    raw: RawPixelBuffer<Box<[u8]>>,
}

impl PixelBuffer {
    /// Creates a pixel buffer from a byte array. The byte array will be copied
    /// into the pixel buffer.
    #[must_use]
    pub fn from_bytes(
        bytes: &[u8],
        width: u32,
        format: PixelFormat,
        color_space: ColorSpace,
    ) -> Self {
        Self {
            raw: RawPixelBuffer {
                format,
                color_space,
                width,
                bytes: bytes.into(),
            },
        }
    }

    #[must_use]
    pub fn from_colors(
        colors: &[Color],
        width: u32,
        format: PixelFormat,
        color_space: ColorSpace,
    ) -> Self {
        let bytes_per_pixel = format.bytes_per_pixel();

        // Initialize bytes this way to avoid zeroing the vec only to overwrite
        // it with the colors anyway.
        let bytes = {
            let mut bytes = Vec::with_capacity(bytes_per_pixel * colors.len());

            for (color, dst) in colors
                .iter()
                .zip(bytes.spare_capacity_mut().chunks_exact_mut(bytes_per_pixel))
            {
                let (count, bytes) = format.write_color(*color);
                debug_assert_eq!(count as usize, dst.len());

                // SAFETY: dst.len() == count
                unsafe {
                    dst.as_mut_ptr()
                        .copy_from_nonoverlapping(bytes.as_ptr().cast(), count as usize);
                }
            }

            // SAFETY: we just copied the bytes into the vec
            unsafe {
                bytes.set_len(bytes_per_pixel * colors.len());
            }
            bytes
        };

        Self {
            raw: RawPixelBuffer {
                format,
                color_space,
                width,
                bytes: bytes.into(),
            },
        }
    }

    #[inline]
    #[must_use]
    pub fn format(&self) -> PixelFormat {
        self.raw.format
    }

    #[inline]
    #[must_use]
    pub fn color_space(&self) -> ColorSpace {
        self.raw.color_space
    }

    #[inline]
    #[must_use]
    pub fn width(&self) -> u32 {
        self.raw.width
    }

    #[inline]
    #[must_use]
    pub fn height(&self) -> u32 {
        self.raw.height()
    }

    #[inline]
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.raw.bytes.as_ref()
    }

    #[inline]
    #[must_use]
    pub fn rows(&self) -> RowIter {
        self.raw.rows()
    }

    #[must_use]
    pub fn as_ref(&self) -> PixelBufferRef {
        self.into()
    }
}

/// A pixel buffer representation over a slice of pixels.
#[derive(Clone, Copy)]
#[allow(clippy::module_name_repetitions)]
pub struct PixelBufferRef<'a> {
    raw: RawPixelBuffer<&'a [u8]>,
}

impl<'a> PixelBufferRef<'a> {
    /// Creates a pixel buffer from a byte array. The byte array will be copied
    /// into the pixel buffer.
    #[must_use]
    pub fn from_bytes(
        bytes: &'a [u8],
        width: u32,
        format: PixelFormat,
        color_space: ColorSpace,
    ) -> Self {
        let row_stride = width as usize * format.bytes_per_pixel();
        assert_eq!(bytes.len() % row_stride, 0);

        Self {
            raw: RawPixelBuffer {
                format,
                color_space,
                width,
                bytes,
            },
        }
    }

    #[inline]
    #[must_use]
    pub fn format(&self) -> PixelFormat {
        self.raw.format
    }

    #[inline]
    #[must_use]
    pub fn color_space(&self) -> ColorSpace {
        self.raw.color_space
    }

    #[inline]
    #[must_use]
    pub fn width(&self) -> u32 {
        self.raw.width
    }

    #[inline]
    #[must_use]
    pub fn height(&self) -> u32 {
        self.raw.height()
    }

    #[inline]
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.raw.bytes
    }

    #[inline]
    #[must_use]
    pub fn rows(&self) -> RowIter {
        self.raw.rows()
    }
}

impl<'a> From<&'a PixelBuffer> for PixelBufferRef<'a> {
    #[inline]
    fn from(pixel_buffer: &'a PixelBuffer) -> Self {
        Self {
            raw: RawPixelBuffer {
                format: pixel_buffer.raw.format,
                color_space: pixel_buffer.raw.color_space,
                width: pixel_buffer.raw.width,
                bytes: pixel_buffer.raw.bytes.as_ref(),
            },
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

/// The actual implementation, generic over the storage mechanism.
#[derive(Clone, Copy)]
struct RawPixelBuffer<T>
where
    T: Deref<Target = [u8]> + Clone,
{
    format: PixelFormat,
    color_space: ColorSpace,
    width: u32,
    bytes: T,
}

impl<T> RawPixelBuffer<T>
where
    T: Deref<Target = [u8]> + Clone,
{
    #[inline]
    fn height(&self) -> u32 {
        let row_size = self.width as usize * self.format.bytes_per_pixel();
        let num_rows = self.bytes.len() / row_size;
        u32::try_from(num_rows).expect("checked cast from usize to u32")
    }

    #[inline]
    fn rows(&self) -> RowIter {
        let row_pitch = self.width as usize * self.format.bytes_per_pixel();
        RowIter {
            row_pitch,
            cursor: 0,
            bytes: self.bytes.as_ref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        let colors = [Color::RED, Color::GREEN, Color::BLUE, Color::WHITE];
        let buffer = PixelBuffer::from_colors(&colors, 2, PixelFormat::Rgba8, ColorSpace::Srgb);

        assert_eq!(buffer.width(), 2);
        assert_eq!(buffer.height(), 2);

        {
            let mut it = buffer.rows();
            assert_eq!(
                it.next(),
                Some([255u8, 0, 0, 255, 0, 255, 0, 255].as_slice())
            );
            assert_eq!(
                it.next(),
                Some([0, 0, 255, 255, 255, 255, 255, 255].as_slice())
            );
            assert_eq!(it.next(), None);
        }

        {
            let buffer_ref: PixelBufferRef = (&buffer).into();
            assert_eq!(buffer_ref.format(), buffer.format());
            assert_eq!(buffer_ref.color_space(), buffer.color_space());
            assert_eq!(buffer_ref.width(), buffer.width());
            assert_eq!(buffer_ref.height(), buffer.height());
            assert_eq!(buffer_ref.bytes(), buffer.bytes());
        }

        {
            let buffer_ref: PixelBufferRef = buffer.as_ref();
            assert_eq!(buffer_ref.format(), buffer.format());
            assert_eq!(buffer_ref.color_space(), buffer.color_space());
            assert_eq!(buffer_ref.width(), buffer.width());
            assert_eq!(buffer_ref.height(), buffer.height());
            assert_eq!(buffer_ref.bytes(), buffer.bytes());
        }
    }
}
