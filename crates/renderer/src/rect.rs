use geometry::{Extent, Point, Px};

use crate::color::Color;

pub enum Part<T> {
    Left(T),
    Right(T),
    Top(T),
    Bottom(T),
    TopLeft(T),
    TopRight(T),
    BottomLeft(T),
    BottomRight(T),
}

pub use Part::*;

pub struct Rect {
    pub position: Point<f32, Px>,
    pub rect_size: Extent<f32, Px>,
    pub outer_radii: [f32; 4],
    pub colors: [Color; 4],
}

impl Rect {
    pub fn new(rect: geometry::Rect<f32, Px>) -> Self {
        Self {
            position: rect.top_left(),
            rect_size: rect.extent(),
            outer_radii: [0.0; 4],
            colors: [Color::BLACK; 4],
        }
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.colors = [color; 4];
        self
    }

    pub fn with_colors<const N: usize>(mut self, colors: [Part<Color>; N]) -> Self {
        for part in colors {
            match part {
                Left(color) => {
                    self.colors[0] = color;
                    self.colors[3] = color;
                }
                Right(color) => {
                    self.colors[1] = color;
                    self.colors[2] = color;
                }
                Top(color) => {
                    self.colors[0] = color;
                    self.colors[1] = color;
                }
                Bottom(color) => {
                    self.colors[2] = color;
                    self.colors[3] = color;
                }
                TopLeft(color) => self.colors[0] = color,
                TopRight(color) => self.colors[1] = color,
                BottomLeft(color) => self.colors[2] = color,
                BottomRight(color) => self.colors[3] = color,
            }
        }

        self
    }

    pub fn with_radius(mut self, radius: f32) -> Self {
        self.outer_radii = [radius; 4];
        self
    }

    pub fn with_radii(mut self, radii: [Part<f32>; 4]) -> Self {
        for part in radii {
            match part {
                Left(radius) => {
                    self.outer_radii[0] = radius;
                    self.outer_radii[3] = radius;
                }
                Right(radius) => {
                    self.outer_radii[1] = radius;
                    self.outer_radii[2] = radius;
                }
                Top(radius) => {
                    self.outer_radii[0] = radius;
                    self.outer_radii[1] = radius;
                }
                Bottom(radius) => {
                    self.outer_radii[2] = radius;
                    self.outer_radii[3] = radius;
                }
                TopLeft(radius) => self.outer_radii[0] = radius,
                TopRight(radius) => self.outer_radii[1] = radius,
                BottomLeft(radius) => self.outer_radii[2] = radius,
                BottomRight(radius) => self.outer_radii[3] = radius,
            }
        }

        self
    }
}
