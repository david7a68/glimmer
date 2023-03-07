use num_traits::Num;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point<T>
where
    T: Num + Copy,
{
    pub x: T,
    pub y: T,
}

impl<T> Point<T>
where
    T: Num + Copy,
{
    #[must_use]
    pub fn zero() -> Self {
        Self {
            x: T::zero(),
            y: T::zero(),
        }
    }

    #[must_use]
    pub fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl<T> Default for Point<T>
where
    T: Num + Copy,
{
    fn default() -> Self {
        Self::zero()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Offset<T>
where
    T: Num + Copy,
{
    pub dx: T,
    pub dy: T,
}

impl<T> Offset<T>
where
    T: Num + Copy,
{
    #[must_use]
    pub fn zero() -> Self {
        Self {
            dx: T::zero(),
            dy: T::zero(),
        }
    }
}

impl<T> Default for Offset<T>
where
    T: Num + Copy,
{
    fn default() -> Self {
        Self::zero()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extent<T>
where
    T: Num + Copy,
{
    pub width: T,
    pub height: T,
}

impl<T> Extent<T>
where
    T: Num + Copy,
{
    #[must_use]
    pub fn zero() -> Self {
        Self {
            width: T::zero(),
            height: T::zero(),
        }
    }

    #[must_use]
    pub fn new(width: T, height: T) -> Self {
        Self { width, height }
    }
}

impl<T> Default for Extent<T>
where
    T: Num + Copy,
{
    fn default() -> Self {
        Self::zero()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect<P, E>
where
    P: Num + Copy,
    E: Num + Copy,
{
    pub origin: Point<P>,
    pub size: Extent<E>,
}

impl<P, E> Rect<P, E>
where
    P: Num + Copy,
    E: Num + Copy,
{
    #[must_use]
    pub fn zero() -> Self {
        Self {
            origin: Point::zero(),
            size: Extent::zero(),
        }
    }

    #[must_use]
    pub fn new(origin: Point<P>, size: Extent<E>) -> Self {
        Self { origin, size }
    }
}

impl<P, E> Default for Rect<P, E>
where
    P: Num + Copy,
    E: Num + Copy,
{
    fn default() -> Self {
        Self::zero()
    }
}
