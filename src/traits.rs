//! Contains the definitions of the various traits used in this crate.

use num_traits::{NumRef, RefNum, NumCast, NumOps, Zero, NumAssign};
use std::fmt::{Debug, Display};

/// Implemented for primitive pixel types.
pub trait Primitive: Copy + Clone + Debug + Display + NumAssign + RefNum<Self> + NumCast + PartialOrd { }

impl<T> Primitive for T
    where T: Copy + Clone + Debug + Display + NumAssign + RefNum<T> + NumCast + PartialOrd
{}

/// This trait must be implemented for the types you want to store in an image.
pub trait Pixel: Clone + PartialEq {
    type Subpixel: Clone;

    /// Number of channels contained in the pixel type
    const N_CHANNELS: u32;

    /// Return a slice containing the different channels of the pixel.
    fn channels(&self) -> &[Self::Subpixel];

    /// Return a mutable slice containing the different channels of the pixel.
    fn channels_mut(&mut self) -> &mut [Self::Subpixel];

    /// Create a new pixel from a slice.
    ///
    /// **Panics**: the length of the slice is not checked, so this function will panic if s.len()
    /// is less than the number of channels in the pixel.
    fn from_slice(s: &[Self::Subpixel]) -> Self;

    /// Set the value of the pixel from a slice.
    ///
    /// **Panics**: the length of the slice is not checked, so this function will panic if s.len()
    /// is less than the number of channels in the pixel.
    fn set_to_slice(&mut self, s: &[Self::Subpixel]);

    fn map<F>(&self, f: F) -> Self
        where F: Fn(Self::Subpixel) -> Self::Subpixel;

    fn sum(&self) -> Self::Subpixel
        where Self::Subpixel: Primitive
    {
        self.channels().iter().fold(Self::Subpixel::zero(), |s1, s2| s1 + *s2)
    }
}

/// Trait for types representing image regions.
pub trait Region {
    /// Return `true` if the region contains the specified point, `false` otherwise.
    fn contains(&self, x: u32, y: u32) -> bool;
}

/// Marker trait for pixel types that overload arithmetic operations.
pub trait PixelOps: Pixel + NumOps { }

/// Marker trait for borrowed pixel types that overload arithmetic operations.
pub trait PixelOpsRef: PixelOps + NumRef { }

/// Enables casts between pixel types.
///
/// Rust's type system can't (AFAIK) cannot express that both pixel types should have the same
/// number of channels, so this restriction is not enforced. However, all implementations of this
/// trait by pixel within this crate are bounded to only cast between related pixel types only
/// differing by their subpixel associated type. If you're implementing your own pixel types,
/// you should probably do the same.
pub trait PixelCast<P, S, O>: Pixel<Subpixel=S>
    where P: Pixel<Subpixel=O>,
          O: NumCast + Clone,
          S: NumCast + Clone
{
    /// Cast `other` into Self and assign the value to self.
    fn cast_from(&mut self, other: &P);

    /// Cast self into P and assign the value to `other`.
    fn cast_to(&self, other: &mut P);
}
