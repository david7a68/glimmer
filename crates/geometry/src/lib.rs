use std::ops::{Add, AddAssign, Div, Sub};

use euclid::num::One;
pub use euclid::{Point2D as Point, Size2D as Extent, Vector2D as Offset};

#[derive(Clone, Copy)]
pub struct Px();

#[derive(Clone, Copy)]
pub struct ScreenPx();

#[derive(Clone, Copy)]
pub struct Rect<T, U>(euclid::Box2D<T, U>);

impl<T, U> Rect<T, U> {
    pub fn new(origin: Point<T, U>, extent: Extent<T, U>) -> Self
    where
        T: Copy + Add<T, Output = T>,
    {
        Self(euclid::Box2D::from_origin_and_size(origin, extent))
    }

    pub fn top_left(&self) -> Point<T, U>
    where
        T: Copy,
    {
        self.0.min
    }

    pub fn bottom_right(&self) -> Point<T, U>
    where
        T: Copy,
    {
        self.0.max
    }

    pub fn top_right(&self) -> Point<T, U>
    where
        T: Copy + Add<T, Output = T>,
    {
        Point::new(self.0.max.x, self.0.min.y)
    }

    pub fn bottom_left(&self) -> Point<T, U>
    where
        T: Copy + Add<T, Output = T>,
    {
        Point::new(self.0.min.x, self.0.max.y)
    }

    pub fn extent(&self) -> Extent<T, U>
    where
        T: Copy + Sub<T, Output = T>,
    {
        self.0.size()
    }

    pub fn center(&self) -> Point<T, U>
    where
        T: Copy + One + Add<Output = T> + Div<Output = T>,
    {
        self.0.center()
    }

    pub fn intersection(&self, rhs: &Rect<T, U>) -> Option<Rect<T, U>>
    where
        T: Copy + PartialOrd,
    {
        self.0.intersection(&rhs.0).map(|r| Rect(r))
    }
}

impl<T, U> Add<Offset<T, U>> for Rect<T, U>
where
    T: Copy + Add<T, Output = T>,
{
    type Output = Self;

    fn add(self, rhs: Offset<T, U>) -> Self::Output {
        Self(self.0.translate(rhs))
    }
}

impl<T, U> AddAssign<Offset<T, U>> for Rect<T, U>
where
    T: Copy + Add<T, Output = T>,
{
    fn add_assign(&mut self, rhs: Offset<T, U>) {
        self.0 = self.0.translate(rhs);
    }
}
