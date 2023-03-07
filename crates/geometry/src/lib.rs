use std::{
    marker::PhantomData,
    ops::{Mul, Sub},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UndefinedUnit;

/// The finite 2D coordinate space of the actual render target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenSpace;

/// The infinite 2D coordinate space where objects are placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldSpace;

/// The object space is a per-object 2D coordinate system where the Y-axis
/// points down. The origin can be changed by translating the object's
/// vertices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectSpace;

/// The clip space is a 2D plane from -1 to 1 in both the X and Y axis. The Y
/// axis points up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipSpace;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Radians<T: num::Real>(pub T);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Degrees<T: num::Real>(pub T);

pub trait Angle<T: num::Real>: Mul<T> {
    fn to_radians(&self) -> Radians<T>;
    fn to_degrees(&self) -> Degrees<T>;
    fn sin(&self) -> T;
    fn cos(&self) -> T;
    fn sin_cos(&self) -> (T, T);
}

impl<T: num::Real> Mul<T> for Radians<T> {
    type Output = Radians<T>;

    fn mul(self, rhs: T) -> Self::Output {
        Radians(self.0 * rhs)
    }
}

impl<T: num::Real> Angle<T> for Radians<T> {
    fn to_radians(&self) -> Radians<T> {
        *self
    }

    fn to_degrees(&self) -> Degrees<T> {
        Degrees(self.0.radians_to_degrees())
    }

    fn sin(&self) -> T {
        self.0.sin()
    }

    fn cos(&self) -> T {
        self.0.cos()
    }

    fn sin_cos(&self) -> (T, T) {
        self.0.sin_cos()
    }
}

impl<T: num::Real> Mul<T> for Degrees<T> {
    type Output = Degrees<T>;

    fn mul(self, rhs: T) -> Self::Output {
        Degrees(self.0 * rhs)
    }
}

impl<T: num::Real> Angle<T> for Degrees<T> {
    fn to_radians(&self) -> Radians<T> {
        Radians(self.0.degrees_to_radians())
    }

    fn to_degrees(&self) -> Degrees<T> {
        *self
    }

    fn sin(&self) -> T {
        self.0.sin()
    }

    fn cos(&self) -> T {
        self.0.cos()
    }

    fn sin_cos(&self) -> (T, T) {
        self.0.sin_cos()
    }
}

#[repr(C)]
#[derive(Copy, Debug, PartialEq, Eq)]
pub struct Point<T, Unit = UndefinedUnit> {
    pub x: T,
    pub y: T,
    _unit: PhantomData<Unit>,
}

impl<T, Unit> Clone for Point<T, Unit>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            x: self.x.clone(),
            y: self.y.clone(),
            _unit: PhantomData,
        }
    }
}

impl<T, Unit> Point<T, Unit> {
    pub fn new(x: T, y: T) -> Self {
        Self {
            x,
            y,
            _unit: PhantomData,
        }
    }

    pub fn zero() -> Self
    where
        T: num::Zero,
    {
        Self::new(T::zero(), T::zero())
    }

    pub fn one() -> Self
    where
        T: num::One,
    {
        Self::new(T::one(), T::one())
    }
}

impl<T: Default, Unit> Default for Point<T, Unit> {
    fn default() -> Self {
        Self::new(T::default(), T::default())
    }
}

impl<T: Sub, Unit> Sub for Point<T, Unit> {
    type Output = Offset<T::Output, Unit>;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Output::new(self.x - rhs.x, self.y - rhs.y)
    }
}

#[repr(C)]
pub struct Offset<T, Unit = UndefinedUnit> {
    pub x: T,
    pub y: T,
    _unit: PhantomData<Unit>,
}

impl<T, Unit> Offset<T, Unit> {
    pub fn new(x: T, y: T) -> Self {
        Self {
            x,
            y,
            _unit: PhantomData,
        }
    }

    pub fn zero() -> Self
    where
        T: num::Zero,
    {
        Self::new(T::zero(), T::zero())
    }

    pub fn one() -> Self
    where
        T: num::One,
    {
        Self::new(T::one(), T::one())
    }
}

impl<T, Unit> From<Point<T, Unit>> for Offset<T, Unit> {
    fn from(point: Point<T, Unit>) -> Self {
        Self {
            x: point.x,
            y: point.y,
            _unit: PhantomData,
        }
    }
}

impl<T: Default, Unit> Default for Offset<T, Unit> {
    fn default() -> Self {
        Self::new(T::default(), T::default())
    }
}

#[repr(C)]
pub struct Scale<T, Unit = UndefinedUnit> {
    pub x: T,
    pub y: T,
    _unit: PhantomData<Unit>,
}

impl<T, Unit> Scale<T, Unit> {
    pub fn new(x: T, y: T) -> Self {
        Self {
            x,
            y,
            _unit: PhantomData,
        }
    }

    pub fn zero() -> Self
    where
        T: num::Zero,
    {
        Self::new(T::zero(), T::zero())
    }

    pub fn one() -> Self
    where
        T: num::One,
    {
        Self::new(T::one(), T::one())
    }
}

impl<T: Default, Unit> Default for Scale<T, Unit> {
    fn default() -> Self {
        Self::new(T::default(), T::default())
    }
}

#[repr(C)]
pub struct Extent<T, Unit = UndefinedUnit> {
    pub width: T,
    pub height: T,
    _unit: PhantomData<Unit>,
}

impl<T, Unit> Extent<T, Unit> {
    pub fn new(width: T, height: T) -> Self {
        Self {
            width,
            height,
            _unit: PhantomData,
        }
    }

    pub fn zero() -> Self
    where
        T: num::Zero,
    {
        Self::new(T::zero(), T::zero())
    }
}

impl<T: Default, Unit> Default for Extent<T, Unit> {
    fn default() -> Self {
        Self::new(T::default(), T::default())
    }
}

#[repr(C)]
pub struct Rect<T, Unit = UndefinedUnit> {
    pub p0: Point<T, Unit>,
    pub p1: Point<T, Unit>,
}

impl<T, Unit> Rect<T, Unit> {
    pub fn new(p0: Point<T, Unit>, p1: Point<T, Unit>) -> Self {
        Self { p0, p1 }
    }

    pub fn zero() -> Self
    where
        T: num::Zero,
    {
        Self::new(Point::zero(), Point::zero())
    }

    pub fn extent(&self) -> Extent<T, Unit>
    where
        T: Sub<Output = T> + Copy,
    {
        Extent::new(self.p1.x - self.p0.x, self.p1.y - self.p0.y)
    }
}

/// A 2D transform stored as a 3x3 matrix in column-major order compressed into
/// a 2x3 matrix.
///
/// ```text
/// | m11 m12 0 |
/// | m21 m22 0 |
/// | m31 m32 1 |
/// ```
#[repr(C)]
#[derive(Copy)]
pub struct Transform<T: num::Real, Src = UndefinedUnit, Dst = UndefinedUnit> {
    pub m11: T,
    pub m12: T,
    pub m21: T,
    pub m22: T,
    pub m31: T,
    pub m32: T,
    _src: PhantomData<Src>,
    _dst: PhantomData<Dst>,
}

impl<T: num::Real, Src, Dst> Transform<T, Src, Dst> {
    pub fn identity() -> Self {
        Self {
            m11: T::one(),
            m12: T::zero(),
            m21: T::zero(),
            m22: T::one(),
            m31: T::zero(),
            m32: T::zero(),
            _src: PhantomData,
            _dst: PhantomData,
        }
    }

    pub fn translate(offset: Offset<T, Dst>) -> Self {
        Self {
            m11: T::one(),
            m12: T::zero(),
            m21: T::zero(),
            m22: T::one(),
            m31: offset.x,
            m32: offset.y,
            _src: PhantomData,
            _dst: PhantomData,
        }
    }

    pub fn scale(scale: Scale<T, Dst>) -> Self {
        Self {
            m11: scale.x,
            m12: T::zero(),
            m21: T::zero(),
            m22: scale.y,
            m31: T::zero(),
            m32: T::zero(),
            _src: PhantomData,
            _dst: PhantomData,
        }
    }

    pub fn rotate<A: Angle<T>>(angle: A) -> Self {
        let (s, c) = angle.sin_cos();
        Self {
            m11: c,
            m12: s,
            m21: -s,
            m22: c,
            m31: T::zero(),
            m32: T::zero(),
            _src: PhantomData,
            _dst: PhantomData,
        }
    }

    pub fn then<NewDst>(&self, rhs: &Transform<T, Dst, NewDst>) -> Transform<T, Src, NewDst> {
        Transform {
            m11: self.m11 * rhs.m11 + self.m12 * rhs.m21,
            m12: self.m11 * rhs.m12 + self.m12 * rhs.m22,
            m21: self.m21 * rhs.m11 + self.m22 * rhs.m21,
            m22: self.m21 * rhs.m12 + self.m22 * rhs.m22,
            m31: self.m31 * rhs.m11 + self.m32 * rhs.m21 + rhs.m31,
            m32: self.m31 * rhs.m12 + self.m32 * rhs.m22 + rhs.m32,
            _src: PhantomData,
            _dst: PhantomData,
        }
    }

    pub fn then_translate(&self, offset: Offset<T, Dst>) -> Transform<T, Src, Dst> {
        self.then(&Transform::translate(offset))
    }

    pub fn translate_then(&self, offset: Offset<T, Src>) -> Self {
        Transform::translate(offset).then(self)
    }

    pub fn then_rotate<A: Angle<T>>(&self, angle: A) -> Self {
        self.then(&Transform::rotate(angle))
    }

    pub fn rotate_then<A: Angle<T>>(&self, angle: A) -> Self {
        Transform::rotate(angle).then(self)
    }

    pub fn then_scale(&self, scale: Scale<T, Dst>) -> Self {
        self.then(&Transform::scale(scale))
    }

    pub fn scale_then(&self, scale: Scale<T, Src>) -> Self {
        Transform::scale(scale).then(self)
    }

    pub fn transform_point(&self, point: Point<T, Src>) -> Point<T, Dst> {
        Point::new(
            self.m11 * point.x + self.m12 * point.y + self.m31,
            self.m21 * point.x + self.m22 * point.y + self.m32,
        )
    }

    pub fn with_src<NewSrc>(&self) -> Transform<T, NewSrc, Dst> {
        Transform {
            m11: self.m11,
            m12: self.m12,
            m21: self.m21,
            m22: self.m22,
            m31: self.m31,
            m32: self.m32,
            _src: PhantomData,
            _dst: PhantomData,
        }
    }

    pub fn with_dst<NewDst>(&self) -> Transform<T, Src, NewDst> {
        Transform {
            m11: self.m11,
            m12: self.m12,
            m21: self.m21,
            m22: self.m22,
            m31: self.m31,
            m32: self.m32,
            _src: PhantomData,
            _dst: PhantomData,
        }
    }

    pub fn as_matrix4x4(&self) -> [[T; 4]; 4] {
        [
            [self.m11, self.m12, T::zero(), T::zero()],
            [self.m21, self.m22, T::zero(), T::zero()],
            [self.m31, self.m32, T::one(), T::zero()],
            [T::zero(), T::zero(), T::zero(), T::one()],
        ]
    }
}

impl<T: num::Real, Src, Dst> Clone for Transform<T, Src, Dst> {
    fn clone(&self) -> Self {
        Self {
            m11: self.m11,
            m12: self.m12,
            m21: self.m21,
            m22: self.m22,
            m31: self.m31,
            m32: self.m32,
            _src: PhantomData,
            _dst: PhantomData,
        }
    }
}

impl<T: num::Real, Src, Dst> Default for Transform<T, Src, Dst> {
    fn default() -> Self {
        Self::identity()
    }
}

impl<T: num::Real, Src, Dst> std::fmt::Debug for Transform<T, Src, Dst> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "Transform {{ m11: {}, m12: {}, m21: {}, m22: {}, m31: {}, m32: {} }}",
            self.m11, self.m12, self.m21, self.m22, self.m31, self.m32
        )
    }
}

pub mod num {
    use std::ops::{Add, Mul, Neg};

    pub trait Zero {
        fn zero() -> Self;
    }

    pub trait One {
        fn one() -> Self;
    }

    pub trait Real:
        Zero
        + One
        + Sized
        + Copy
        + Default
        + Neg<Output = Self>
        + Add<Output = Self>
        + Mul<Output = Self>
        + PartialOrd
        + std::fmt::Debug
        + std::fmt::Display
        + Into<f32>
    {
        fn sin(self) -> Self;
        fn cos(self) -> Self;
        fn sin_cos(self) -> (Self, Self);
        fn radians_to_degrees(self) -> Self;
        fn degrees_to_radians(self) -> Self;
    }

    impl Zero for i32 {
        fn zero() -> Self {
            0
        }
    }

    impl One for i32 {
        fn one() -> Self {
            1
        }
    }

    impl Zero for f32 {
        fn zero() -> Self {
            0.0
        }
    }

    impl One for f32 {
        fn one() -> Self {
            1.0
        }
    }

    impl Real for f32 {
        fn sin(self) -> Self {
            self.sin()
        }

        fn cos(self) -> Self {
            self.cos()
        }

        fn sin_cos(self) -> (Self, Self) {
            self.sin_cos()
        }

        fn radians_to_degrees(self) -> Self {
            self * 180.0 / std::f32::consts::PI
        }

        fn degrees_to_radians(self) -> Self {
            self * std::f32::consts::PI / 180.0
        }
    }
}
