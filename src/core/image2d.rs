//! Defines a generic 2D image type.
#![allow(unknown_lints)]

use core::{Luma, LumaA, Pixel, PixelType, Primitive, Rect, Rgb, RgbA};

use failure::Error;
use ndarray;
use ndarray::prelude::*;
use ndarray::{OwnedRepr, ViewRepr};
use num_traits::Zero;
#[cfg(feature = "rand_integration")]
use rand::{
    distributions::{Distribution, Standard},
    Rng,
};

use std::cmp::min;
use std::iter::{DoubleEndedIterator, ExactSizeIterator, IntoIterator};
use std::ops::{Add, Div, Index, IndexMut, Mul, Rem, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Bit depth of an image.
pub enum BitDepth {
    /// 8 bit image.
    _8,
    /// 16 bit image.
    _16,
}

/// Type of an image.
pub type ImageType = (PixelType, BitDepth);

/// 2-dimensional image type.
pub trait Image2D<P>: Sync
where
    P: Pixel,
{
    /// Return a slice if the view points to contiguous memory in standard order.
    fn as_slice(&self) -> Option<&[P]>;

    /// Return the pixel at the specified coordinates.
    ///
    /// **Panics** if the index is out of bounds.
    fn get_pixel(&self, x: u32, y: u32) -> &P;

    /// Return a view on the image.
    fn get_view(&self) -> Image2DView<P>;

    /// Return the width of the image.
    fn width(&self) -> u32;
    /// Return the height of the image.
    fn height(&self) -> u32;
    /// Return the dimensions of the image as a `(width, height)` tuple.
    fn dimensions(&self) -> (u32, u32) {
        (self.width(), self.height())
    }

    // TODO: map to u32's for coherence
    /// Return an iterator to the pixels and their indices. The type of the iterator is ((usize, usize), &P)
    fn enumerate_pixels(&self) -> ndarray::iter::IndexedIter<P, Ix2>;

    /// Return an iterator over the pixels of an image row in left to right order.
    fn row(&self, y: u32) -> Option<RowIter<P>>;

    /// Return an iterator over the rows of an image in scanline order.
    fn rows(&self) -> RowsIter<P>;

    /// Return an iterator over the pixels of an image column in scanline order.
    fn col(&self, x: u32) -> Option<ColIter<P>>;

    /// Return an iterator over the columns of an image in left to right order.
    fn cols(&self) -> ColsIter<P>;

    /// Return a Rect containing the whole image.
    fn rect(&self) -> Rect {
        Rect::new(0, 0, self.width(), self.height())
    }

    /// Return an iterator on a subset of the image of specified dimensions starting at the specified coordinates.
    ///
    /// **Panics** if the specified region crosses image boundaries.
    fn rect_iter(&self, rect: Rect) -> RectIter<P>;

    /// Translate the given `Rect` within the image by the given 2D vector. The parts of the original `Rect` than fall
    /// out of the iamge will be cropped. Return the translated `Rect` if it's not empty, or `None` otherwise.
    fn translate_rect(&self, rect: Rect, x: i64, y: i64) -> Option<Rect> {
        let left = i64::from(rect.left()) + x;
        let top = i64::from(rect.top()) + y;
        let right = i64::from(rect.right()) + x;
        let bottom = i64::from(rect.bottom()) + y;
        let (w_signed, h_signed) = (i64::from(self.width()), i64::from(self.height()));

        // Detect early if the resulting rectangle falls out of the image
        if left < w_signed && top < h_signed && right >= 0 && bottom >= 0 {
            // Compute the new Rect
            let x_left = if left < 0 { 0 } else { left as u32 };
            let y_top = if top < 0 { 0 } else { top as u32 };
            Some(Rect::new(
                x_left,
                y_top,
                (min(w_signed, right + 1) as u32) - x_left,
                (min(h_signed, bottom + 1) as u32) - y_top,
            ))
        } else {
            None
        }
    }

    /// Return an Iterator over the image pixels.
    fn iter(&self) -> Iter<P>;

    /// Return an owned copy of the image.
    fn to_owned(&self) -> ImageBuffer2D<P>;

    /// Return a view over a rectangular region of the image.
    fn sub_image(&self, rect: Rect) -> Image2DView<P>;
}

impl<'a, P> IntoIterator for &'a Image2D<P>
where
    P: Pixel + 'a,
{
    type Item = &'a P;
    type IntoIter = Iter<'a, P>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<P> Index<(u32, u32)> for Image2D<P>
where
    P: Pixel,
{
    type Output = P;

    fn index(&self, idx: (u32, u32)) -> &P {
        self.get_pixel(idx.0, idx.1)
    }
}

macro_rules! impl_image_op {
    ($op_name:ident, $op_fn:ident) => {
        // &T op &T impls
        impl<'a, 'b, P> $op_name<&'a Image2DRepr<OwnedRepr<P>, P>>
            for &'b Image2DRepr<OwnedRepr<P>, P>
        where
            P: Pixel + $op_name<P, Output = P>,
        {
            type Output = Result<ImageBuffer2D<P>, Error>;

            fn $op_fn(self, rhs: &'a Image2DRepr<OwnedRepr<P>, P>) -> Self::Output {
                let (dim_lhs, dim_rhs) = (self.dimensions(), rhs.dimensions());
                if dim_lhs != dim_rhs {
                    bail!("Image dimensions do not match");
                }
                Ok(ImageBuffer2D {
                    buffer: (&self.buffer).$op_fn(&rhs.buffer),
                })
            }
        }

        impl<'a, 'b, 'c, P> $op_name<&'a Image2DRepr<ViewRepr<&'b P>, P>>
            for &'c Image2DRepr<OwnedRepr<P>, P>
        where
            P: Pixel + $op_name<P, Output = P>,
        {
            type Output = Result<ImageBuffer2D<P>, Error>;

            fn $op_fn(self, rhs: &'a Image2DRepr<ViewRepr<&'b P>, P>) -> Self::Output {
                let (dim_lhs, dim_rhs) = (self.dimensions(), rhs.dimensions());
                if dim_lhs != dim_rhs {
                    bail!("Image dimensions do not match");
                }
                Ok(ImageBuffer2D {
                    buffer: (&self.buffer).$op_fn(&rhs.buffer),
                })
            }
        }

        impl<'a, 'b, 'c, P> $op_name<&'a Image2DRepr<OwnedRepr<P>, P>>
            for &'b Image2DRepr<ViewRepr<&'c P>, P>
        where
            P: Pixel + $op_name<P, Output = P>,
        {
            type Output = Result<ImageBuffer2D<P>, Error>;

            fn $op_fn(self, rhs: &'a Image2DRepr<OwnedRepr<P>, P>) -> Self::Output {
                let (dim_lhs, dim_rhs) = (self.dimensions(), rhs.dimensions());
                if dim_lhs != dim_rhs {
                    bail!("Image dimensions do not match");
                }
                Ok(ImageBuffer2D {
                    buffer: (&self.buffer).$op_fn(&rhs.buffer),
                })
            }
        }

        impl<'a, 'b, 'c, 'd, P> $op_name<&'a Image2DRepr<ViewRepr<&'b P>, P>>
            for &'c Image2DRepr<ViewRepr<&'d P>, P>
        where
            P: Pixel + $op_name<P, Output = P>,
        {
            type Output = Result<ImageBuffer2D<P>, Error>;

            fn $op_fn(self, rhs: &'a Image2DRepr<ViewRepr<&'b P>, P>) -> Self::Output {
                let (dim_lhs, dim_rhs) = (self.dimensions(), rhs.dimensions());
                if dim_lhs != dim_rhs {
                    bail!("Image dimensions do not match");
                }
                Ok(ImageBuffer2D {
                    buffer: (&self.buffer).$op_fn(&rhs.buffer),
                })
            }
        }

        // Image2D impls
        impl<'a, 'b, P> $op_name<&'a dyn Image2D<P>> for &'b dyn Image2D<P>
        where
            P: Pixel + $op_name<P, Output = P>,
        {
            type Output = Result<ImageBuffer2D<P>, Error>;

            fn $op_fn(self, rhs: &'a Image2D<P>) -> Self::Output {
                let (dim_lhs, dim_rhs) = (self.dimensions(), rhs.dimensions());
                if dim_lhs != dim_rhs {
                    bail!("Image dimensions do not match");
                }
                self.get_view().$op_fn(&rhs.get_view())
            }
        }
    };
}

impl_image_op!(Add, add);
impl_image_op!(Mul, mul);
impl_image_op!(Sub, sub);
impl_image_op!(Div, div);
impl_image_op!(Rem, rem);

/// Contains operations on mutable images.
pub trait Image2DMut<P>: Image2D<P>
where
    P: Pixel,
{
    /// Return a mutable reference to the requested pixel.
    ///
    /// **Panics** if the index is out of bounds.
    fn get_pixel_mut(&mut self, x: u32, y: u32) -> &mut P;

    /// Set the pixel at the specified coordinates to the specified value.
    ///
    /// **Panics** if the index is out of bounds.
    fn put_pixel(&mut self, x: u32, y: u32, pixel: P);

    // TODO: map to u32's for coherence
    /// Return an iterator to the pixels and their indices. The type of the iterator is ((usize, usize), &mut P)
    fn enumerate_pixels_mut(&mut self) -> ndarray::iter::IndexedIterMut<P, Ix2>;

    /// Return an iterator over the pixels of an image row in left to right order.
    fn row_mut(&mut self, y: u32) -> Option<RowIterMut<P>>;

    /// Return a mutable iterator over the rows of an image in scanline order.
    fn rows_mut(&mut self) -> RowsIterMut<P>;

    /// Return an iterator over the pixels of an image column in scanline order.
    fn col_mut(&mut self, x: u32) -> Option<ColIterMut<P>>;

    /// Return a mutable iterator over the columns of an image in left to right order.
    fn cols_mut(&mut self) -> ColsIterMut<P>;

    /// Return a mutable view to a subset of the image of specified dimensions starting at the specified coordinates.
    ///
    /// **Panics** if the specified region crosses image boundaries.
    fn rect_iter_mut(&mut self, rect: Rect) -> RectIterMut<P>;

    /// Fill the image with the given value
    fn fill(&mut self, value: &P);

    /// Fill the given `Rect` with the given value.
    fn fill_rect(&mut self, rect: Rect, value: &P) {
        for pixel in self.rect_iter_mut(rect) {
            *pixel = value.clone();
        }
    }

    /// Blit (i.e. copy) a `Rect` from the source image onto the destination image.
    fn blit_rect(&mut self, src_rect: Rect, dst_rect: Rect, img: &Image2D<P>) -> Result<(), Error>
    where
        Self: ::std::marker::Sized,
    {
        if src_rect.size() != dst_rect.size() {
            let (ws, hs) = src_rect.size();
            let (wd, hd) = dst_rect.size();
            bail!(
                "Rects are not the same size. Source is ({}, {}), destination is ({}, {})",
                ws,
                hs,
                wd,
                hd
            );
        }

        if !src_rect.fits_image(img) {
            bail!("Source rect does not fit source image.");
        }
        if !dst_rect.fits_image(self) {
            bail!("Source rect does not fit destination image.");
        }

        for (src_pixel, dst_pixel) in img.rect_iter(src_rect).zip(self.rect_iter_mut(dst_rect)) {
            *dst_pixel = src_pixel.clone();
        }
        Ok(())
    }

    /// Return a mutable Iterator on the image pixels.
    fn iter_mut(&mut self) -> IterMut<P>;

    /// Return a mutable view on a rectangular region of the image.
    fn sub_image_mut(&mut self, rect: Rect) -> Image2DViewMut<P>;
}

impl<P> IndexMut<(u32, u32)> for Image2DMut<P>
where
    P: Pixel,
    Image2DMut<P>: Index<(u32, u32), Output = P>,
{
    fn index_mut(&mut self, idx: (u32, u32)) -> &mut P {
        self.get_pixel_mut(idx.0, idx.1)
    }
}

impl<'a, P> IntoIterator for &'a mut Image2DMut<P>
where
    P: Pixel + 'a,
{
    type Item = &'a mut P;
    type IntoIter = IterMut<'a, P>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

/// Abstract representation of a 2D image. Can contain owned or borrowed data depending on the type of D.
#[derive(Debug)]
pub struct Image2DRepr<D, P>
where
    P: Pixel,
    D: ndarray::Data<Elem = P>,
{
    buffer: ArrayBase<D, Ix2>,
}

impl<D, P> PartialEq for Image2DRepr<D, P>
where
    P: Pixel,
    D: ndarray::Data<Elem = P>,
{
    fn eq(&self, other: &Image2DRepr<D, P>) -> bool {
        self.dimensions() == other.dimensions() && self.iter().eq(other.iter())
    }
}

unsafe impl<D, P> Sync for Image2DRepr<D, P>
where
    P: Pixel,
    D: ndarray::Data<Elem = P>,
{
}

impl<D, P> Image2D<P> for Image2DRepr<D, P>
where
    P: Pixel,
    D: ndarray::Data<Elem = P>,
{
    fn width(&self) -> u32 {
        self.buffer.cols() as u32
    }

    fn height(&self) -> u32 {
        self.buffer.rows() as u32
    }

    fn dimensions(&self) -> (u32, u32) {
        (self.width(), self.height())
    }

    fn as_slice(&self) -> Option<&[P]> {
        self.buffer.as_slice()
    }

    fn get_pixel(&self, x: u32, y: u32) -> &P {
        &self.buffer[[y as usize, x as usize]]
    }

    fn get_view(&self) -> Image2DView<P> {
        Image2DView {
            buffer: self.buffer.view(),
        }
    }

    // TODO: map to u32's for coherence
    fn enumerate_pixels(&self) -> ndarray::iter::IndexedIter<P, Ix2> {
        self.buffer.indexed_iter()
    }

    fn row(&self, y: u32) -> Option<RowIter<P>> {
        if y < self.height() {
            Some(RowIter {
                iter: self.buffer.slice(s![y as usize, ..]).into_iter(),
            })
        } else {
            None
        }
    }

    fn rows(&self) -> RowsIter<P> {
        RowsIter {
            iter: self.buffer.axis_iter(Axis(0)),
        }
    }

    fn col(&self, x: u32) -> Option<ColIter<P>> {
        if x < self.width() {
            Some(ColIter {
                iter: self.buffer.slice(s![.., x as usize]).into_iter(),
            })
        } else {
            None
        }
    }

    fn cols(&self) -> ColsIter<P> {
        ColsIter {
            iter: self.buffer.axis_iter(Axis(1)),
        }
    }

    fn rect_iter(&self, rect: Rect) -> RectIter<P> {
        let left = rect.left() as isize;
        let top = rect.top() as isize;
        let right = left + rect.width() as isize;
        let bottom = top + rect.height() as isize;

        RectIter {
            iter: self.buffer.slice(s![top..bottom, left..right]).into_iter(),
        }
    }

    fn iter(&self) -> Iter<P> {
        self.buffer.into_iter()
    }

    fn to_owned(&self) -> ImageBuffer2D<P> {
        ImageBuffer2D {
            buffer: self.buffer.to_owned(),
        }
    }

    fn sub_image(&self, rect: Rect) -> Image2DView<P> {
        Image2DRepr {
            buffer: self.buffer.slice(s![
                rect.top() as usize..(rect.bottom() + 1) as usize,
                rect.left() as usize..(rect.right() + 1) as usize
            ]),
        }
    }
}

impl<D, P> Index<(u32, u32)> for Image2DRepr<D, P>
where
    P: Pixel,
    D: ndarray::Data<Elem = P>,
{
    type Output = P;

    fn index(&self, idx: (u32, u32)) -> &P {
        self.get_pixel(idx.0, idx.1)
    }
}

impl<D, P> Image2DMut<P> for Image2DRepr<D, P>
where
    P: Pixel,
    D: ndarray::DataMut<Elem = P>,
{
    fn get_pixel_mut(&mut self, x: u32, y: u32) -> &mut P {
        &mut self.buffer[[y as usize, x as usize]]
    }

    fn put_pixel(&mut self, x: u32, y: u32, pixel: P) {
        self.buffer[[y as usize, x as usize]] = pixel;
    }

    fn enumerate_pixels_mut(&mut self) -> ndarray::iter::IndexedIterMut<P, Ix2> {
        self.buffer.indexed_iter_mut()
    }

    fn row_mut(&mut self, y: u32) -> Option<RowIterMut<P>> {
        if y < self.height() {
            Some(RowIterMut {
                iter: self.buffer.slice_mut(s![y as usize, ..]).into_iter(),
            })
        } else {
            None
        }
    }

    fn rows_mut(&mut self) -> RowsIterMut<P> {
        RowsIterMut {
            iter: self.buffer.axis_iter_mut(Axis(0)),
        }
    }

    fn col_mut(&mut self, x: u32) -> Option<ColIterMut<P>> {
        if x < self.width() {
            Some(ColIterMut {
                iter: self.buffer.slice_mut(s![.., x as usize]).into_iter(),
            })
        } else {
            None
        }
    }

    fn cols_mut(&mut self) -> ColsIterMut<P> {
        ColsIterMut {
            iter: self.buffer.axis_iter_mut(Axis(1)),
        }
    }

    fn rect_iter_mut(&mut self, rect: Rect) -> RectIterMut<P> {
        let left = rect.left() as isize;
        let top = rect.top() as isize;
        let right = left + rect.width() as isize;
        let bottom = top + rect.height() as isize;

        RectIterMut {
            iter: self
                .buffer
                .slice_mut(s![top..bottom, left..right])
                .into_iter(),
        }
    }

    fn fill(&mut self, value: &P) {
        self.buffer.fill(value.clone());
    }

    fn iter_mut(&mut self) -> IterMut<P> {
        self.buffer.iter_mut()
    }

    fn sub_image_mut(&mut self, rect: Rect) -> Image2DViewMut<P> {
        Image2DRepr {
            buffer: self.buffer.slice_mut(s![
                rect.top() as usize..(rect.bottom() + 1) as usize,
                rect.left() as usize..(rect.right() + 1) as usize
            ]),
        }
    }
}

impl<D, P> IndexMut<(u32, u32)> for Image2DRepr<D, P>
where
    P: Pixel,
    D: ndarray::DataMut<Elem = P>,
    Image2DRepr<D, P>: Index<(u32, u32), Output = P>,
{
    fn index_mut(&mut self, idx: (u32, u32)) -> &mut P {
        self.get_pixel_mut(idx.0, idx.1)
    }
}

impl<'a, D, P> IntoIterator for &'a Image2DRepr<D, P>
where
    P: Pixel + 'a,
    D: ndarray::Data<Elem = P>,
{
    type Item = &'a P;
    type IntoIter = Iter<'a, P>;

    fn into_iter(self) -> Self::IntoIter {
        self.buffer.into_iter()
    }
}

impl<'a, D, P> IntoIterator for &'a mut Image2DRepr<D, P>
where
    P: Pixel + 'a,
    D: ndarray::DataMut<Elem = P>,
{
    type Item = &'a mut P;
    type IntoIter = IterMut<'a, P>;

    fn into_iter(self) -> Self::IntoIter {
        self.buffer.iter_mut()
    }
}

/// Owned 2D image representation.
pub type ImageBuffer2D<P> = Image2DRepr<ndarray::OwnedRepr<P>, P>;
/// Borrowed 2D image representation.
pub type Image2DView<'a, P> = Image2DRepr<ndarray::ViewRepr<&'a P>, P>;
/// Mutably borrowed 2D image representation.
pub type Image2DViewMut<'a, P> = Image2DRepr<ndarray::ViewRepr<&'a mut P>, P>;

// Type of ndarray iterators.
type Iter<'a, P> = ndarray::iter::Iter<'a, P, Ix2>;
type IterMut<'a, P> = ndarray::iter::IterMut<'a, P, Ix2>;

impl<P> ImageBuffer2D<P>
where
    P: Pixel,
{
    /// Create a new owned image of specified dimensions filled with zeros.
    pub fn new(width: u32, height: u32) -> ImageBuffer2D<P>
    where
        P: Pixel + Zero,
    {
        ImageBuffer2D {
            buffer: Array2::zeros((height as usize, width as usize)),
        }
    }

    /// Consume self and return the raw underlying storage Vec.
    pub fn into_raw_vec(self) -> Vec<P> {
        self.buffer.into_raw_vec()
    }

    /// Create a new image of specified dimensions from a `Vec` of the specified pixel type.
    ///
    /// **Error**: `InvalidDimensions` if the dimensions do not match the length of `v`.
    pub fn from_vec(w: u32, h: u32, v: Vec<P>) -> Result<ImageBuffer2D<P>, Error> {
        let buf = try!(Array2::from_shape_vec((h as usize, w as usize), v));
        Ok(ImageBuffer2D { buffer: buf })
    }

    /// Create a new image of specified dimensions from a `Vec` of the specified pixel type's subpixel.
    ///
    /// **Error**: `InvalidDimensions` if the dimensions do not match the length of `v`.
    pub fn from_raw_vec(w: u32, h: u32, v: &[P::Subpixel]) -> Result<ImageBuffer2D<P>, Error> {
        let pixels_iter = v.chunks(P::N_CHANNELS as usize);
        ensure!(
            pixels_iter.len() == (w * h) as usize,
            "Buffer has incorrect size {}, expected {}.",
            pixels_iter.len(),
            w * h
        );
        let mut v_pixels = vec![];
        for channels in pixels_iter {
            v_pixels.push(P::from_slice(channels))
        }
        let buf = try!(Array2::from_shape_vec((h as usize, w as usize), v_pixels));
        Ok(ImageBuffer2D { buffer: buf })
    }

    /// Generate a new image from a closure that will be called with the index of each pixel.
    pub fn generate<F>(w: u32, h: u32, mut f: F) -> ImageBuffer2D<P>
    where
        F: FnMut((u32, u32)) -> P,
    {
        ImageBuffer2D {
            buffer: Array2::from_shape_fn(Ix2(h as usize, w as usize), |(y, x)| {
                f((x as u32, y as u32))
            }),
        }
    }
}

#[cfg(feature = "rand_integration")]
impl<P> ImageBuffer2D<P>
where
    P: Pixel,
{
    /// Generate a random image with the Standard distribution.
    pub fn rand<R>(width: u32, height: u32, rng: &mut R) -> ImageBuffer2D<P>
    where
        R: Rng,
        Standard: Distribution<P::Subpixel>,
    {
        ImageBuffer2D::generate(width, height, |(_x, _y)| P::rand(rng))
    }

    /// Generate a random image with the given distribution
    pub fn rand_with_distr<D, R>(
        width: u32,
        height: u32,
        rng: &mut R,
        distr: &D,
    ) -> ImageBuffer2D<P>
    where
        R: Rng,
        D: Distribution<P::Subpixel>,
    {
        ImageBuffer2D::generate(width, height, |(_x, _y)| P::rand_with_distr(rng, distr))
    }
}

macro_rules! impl_iterators {
    ( $( $(#[$attr:meta])* $name:ident: $t:ty;)+ ) => {
        $(
        $( #[$attr] )*
        pub struct $name<'a, P>
            where P: Pixel + 'a
        {
            iter: $t
        }

        impl<'a, P> Iterator for $name<'a, P>
            where P: Pixel + 'a
        {
            type Item = <$t as Iterator>::Item;

            fn next(&mut self) -> Option<Self::Item> {
                self.iter.next()
            }
        }

        impl<'a, P> ExactSizeIterator for $name<'a, P>
            where P: Pixel + 'a
        {
            fn len(&self) -> usize {
                self.iter.len()
            }
        }
        )+
    };
}

macro_rules! impl_double_ended_iterators {
    ( $( $(#[$attr:meta])* $name:ident: $t:ty);+ ) => {
        impl_iterators!(
        $(
            $( #[$attr] )*
            $name: $t;
        )+
        );

        $(
        impl<'a, P> DoubleEndedIterator for $name<'a, P>
            where P: Pixel + 'a
        {
            fn next_back(&mut self) -> Option<Self::Item> {
                self.iter.next_back()
            }
        }
        )+
    };
}

impl_iterators!(
    /// Iterator over a rectangular region. Created by `Image2D`'s `rect_iter` method.
    RectIter: Iter<'a, P>;
    /// Mutable iterator over a rectangular region. Created by `Image2DMut`'s `rect_iter_mut` method.
    RectIterMut: IterMut<'a, P>;
);

impl_double_ended_iterators!(
    /// Iterator over the pixels of an image row.
    RowIter: ndarray::iter::Iter<'a, P, Ix1>;
    /// Mutable iterator over the pixels of an image row.
    RowIterMut: ndarray::iter::IterMut<'a, P, Ix1>;
    /// Iterator over the rows of an image.
    RowsIter: ndarray::iter::AxisIter<'a, P, Ix1>;
    /// Mutable iterator over the rows of an image.
    RowsIterMut: ndarray::iter::AxisIterMut<'a, P, Ix1>;
    /// Iterator over the pixels of an image column.
    ColIter: ndarray::iter::Iter<'a, P, Ix1>;
    /// Mutable iterator over the pixels of an image column.
    ColIterMut: ndarray::iter::IterMut<'a, P, Ix1>;
    /// Iterator over the columns of an image.
    ColsIter: ndarray::iter::AxisIter<'a, P, Ix1>;
    /// Mutable iterator over the columns of an image.
    ColsIterMut: ndarray::iter::AxisIterMut<'a, P, Ix1>
);

/// Discard the alpha component of an `RgbA` image.
pub fn rgba_to_rgb<P>(img: &Image2D<RgbA<P>>) -> ImageBuffer2D<Rgb<P>>
where
    P: Primitive,
{
    let mut res = ImageBuffer2D::<Rgb<P>>::new(img.width(), img.height());
    for (src_pixel, dst_pixel) in img.into_iter().zip((&mut res).into_iter()) {
        *dst_pixel = src_pixel.into();
    }
    res
}

/// Discard the alpha component of a `LumaA` image.
pub fn luma_alpha_to_luma<P>(img: &Image2D<LumaA<P>>) -> ImageBuffer2D<Luma<P>>
where
    P: Primitive,
{
    let mut res = ImageBuffer2D::<Luma<P>>::new(img.width(), img.height());
    for (src_pixel, dst_pixel) in img.into_iter().zip((&mut res).into_iter()) {
        *dst_pixel = src_pixel.into();
    }
    res
}

#[cfg(test)]
mod tests {
    use core::{Image2D, Image2DMut, ImageBuffer2D, Luma, Pixel, Rect, Region};

    use num_traits::Zero;
    #[cfg(feature = "rand_integration")]
    use rand::thread_rng;

    use std::fmt::Debug;
    use std::iter::FromIterator;

    #[test]
    fn test_from_vec() {
        let v1 = Vec::from_iter((0u8..9u8).map(|n| Luma::new([n])));
        let v2 = Vec::from_iter((0u8..6u8).map(|n| Luma::new([n])));

        let i1 = ImageBuffer2D::from_vec(3, 3, v1.clone()).unwrap();
        let i2 = ImageBuffer2D::from_vec(2, 3, v2.clone()).unwrap();
        let i3 = ImageBuffer2D::from_vec(3, 2, v2.clone()).unwrap();
        assert_eq!(i1.dimensions(), (3, 3));
        assert_eq!(i2.dimensions(), (2, 3));
        assert_eq!(i3.dimensions(), (3, 2));

        assert!(ImageBuffer2D::from_vec(3, 3, v2.clone()).is_err());
        assert!(ImageBuffer2D::from_vec(4, 2, v2.clone()).is_err());
        for y in 0..3 {
            for x in 0..2 {
                assert_eq!((x + y * 2) as u8, i2.get_pixel(x, y).data[0]);
            }
        }
    }

    #[test]
    fn test_new() {
        fn test_zeros_helper<P>(w: u32, h: u32)
        where
            P: Pixel + Zero + Debug,
        {
            let img = ImageBuffer2D::<P>::new(w, h);
            assert_eq!((w, h), (img.width(), img.height()));
            for pixel in &img {
                assert!(pixel.is_zero());
            }
        };
        test_zeros_helper::<Luma<u8>>(100, 200);
        test_zeros_helper::<Luma<f32>>(100, 200);
    }

    #[test]
    fn test_into_iter() {
        let v: Vec<Luma<u8>> = (1..10).map(Luma::from).collect();
        let img = ImageBuffer2D::from_vec(3, 3, v.clone()).unwrap();

        for (p, i) in img.into_iter().zip(v.into_iter()) {
            assert!(&i == p);
        }
    }

    #[test]
    fn test_enumerate_pixels() {
        let img = ImageBuffer2D::generate(5, 3, |(x, y)| Luma::from((2 * x + 3 * y) as u8));

        for ((x, y), p) in img
            .enumerate_pixels()
            .map(|((y, x), p)| ((x, y), p.channels()[0]))
        {
            assert_eq!((2 * x + 3 * y) as u8, p);
        }
    }

    #[test]
    fn test_add() {
        let v1 = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        let v2 = [8, 7, 6, 5, 4, 3, 2, 1, 0];
        let img1 = ImageBuffer2D::<Luma<u8>>::from_raw_vec(3, 3, &v1).unwrap();
        let img2 = ImageBuffer2D::<Luma<u8>>::from_raw_vec(3, 3, &v2).unwrap();
        let _ = &img1 + &img2;
    }

    #[test]
    fn test_row() {
        let v = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        let img = ImageBuffer2D::<Luma<u8>>::from_raw_vec(3, 3, &v).unwrap();
        let row0 = img.row(0).unwrap();
        let row1 = img.row(1).unwrap();
        let row2 = img.row(2).unwrap();
        let row_out = img.row(3);
        assert_eq!(
            row0.collect::<Vec<&Luma<u8>>>(),
            vec![&Luma::new([0]), &Luma::new([1]), &Luma::new([2])]
        );
        assert_eq!(
            row1.collect::<Vec<&Luma<u8>>>(),
            vec![&Luma::new([3]), &Luma::new([4]), &Luma::new([5])]
        );
        assert_eq!(
            row2.collect::<Vec<&Luma<u8>>>(),
            vec![&Luma::new([6]), &Luma::new([7]), &Luma::new([8])]
        );
        assert!(row_out.is_none());
    }

    #[test]
    fn test_row_mut() {
        let v = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        let mut img = ImageBuffer2D::<Luma<u8>>::from_raw_vec(3, 3, &v).unwrap();
        {
            let row0 = img.row_mut(0).unwrap();
            row0.map(|pix| *pix = Luma::new([pix.data[0] * 2u8]))
                .count();
        }
        {
            let row1 = img.row_mut(1).unwrap();
            row1.map(|pix| *pix = Luma::new([pix.data[0] * 3u8]))
                .count();
        }
        {
            let row2 = img.row_mut(2).unwrap();
            row2.map(|pix| *pix = Luma::new([pix.data[0] * 4u8]))
                .count();
        }
        {
            let row_out = img.row_mut(3);
            assert!(row_out.is_none());
        }
        assert_eq!(
            img.into_raw_vec(),
            vec![
                Luma::new([0]),
                Luma::new([2]),
                Luma::new([4]),
                Luma::new([9]),
                Luma::new([12]),
                Luma::new([15]),
                Luma::new([24]),
                Luma::new([28]),
                Luma::new([32]),
            ]
        );
    }

    #[test]
    fn test_rows() {
        let v = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        let img = ImageBuffer2D::<Luma<u8>>::from_raw_vec(3, 3, &v).unwrap();
        for (y, row) in img.rows().enumerate() {
            for (x, pix) in row.into_iter().enumerate() {
                assert_eq!(pix, &Luma::new([x as u8 + 3 * y as u8]))
            }
        }
    }

    #[test]
    fn test_rows_mut() {
        let v = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        let mut img = ImageBuffer2D::<Luma<u8>>::from_raw_vec(3, 3, &v).unwrap();
        for (y, row) in img.rows().enumerate() {
            for (x, pix) in row.into_iter().enumerate() {
                assert_eq!(pix, &Luma::new([x as u8 + 3 * y as u8]))
            }
        }
        for (y, row) in img.rows_mut().enumerate() {
            for (x, pix) in row.into_iter().enumerate() {
                *pix = Luma::new([3 * x as u8 + 5 * y as u8]);
            }
        }
        for (y, row) in img.rows().enumerate() {
            for (x, pix) in row.into_iter().enumerate() {
                assert_eq!(pix, &Luma::new([3 * x as u8 + 5 * y as u8]))
            }
        }
    }

    #[test]
    fn test_col() {
        let v = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        let img = ImageBuffer2D::<Luma<u8>>::from_raw_vec(3, 3, &v).unwrap();
        let col0 = img.col(0).unwrap();
        let col1 = img.col(1).unwrap();
        let col2 = img.col(2).unwrap();
        let col_out = img.col(3);
        assert_eq!(
            col0.collect::<Vec<&Luma<u8>>>(),
            vec![&Luma::new([0]), &Luma::new([3]), &Luma::new([6])]
        );
        assert_eq!(
            col1.collect::<Vec<&Luma<u8>>>(),
            vec![&Luma::new([1]), &Luma::new([4]), &Luma::new([7])]
        );
        assert_eq!(
            col2.collect::<Vec<&Luma<u8>>>(),
            vec![&Luma::new([2]), &Luma::new([5]), &Luma::new([8])]
        );
        assert!(col_out.is_none());
    }

    #[test]
    fn test_col_mut() {
        let v = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        let mut img = ImageBuffer2D::<Luma<u8>>::from_raw_vec(3, 3, &v).unwrap();
        {
            let col0 = img.col_mut(0).unwrap();
            col0.map(|pix| *pix = Luma::new([pix.data[0] * 2u8]))
                .count();
        }
        {
            let col1 = img.col_mut(1).unwrap();
            col1.map(|pix| *pix = Luma::new([pix.data[0] * 3u8]))
                .count();
        }
        {
            let col2 = img.col_mut(2).unwrap();
            col2.map(|pix| *pix = Luma::new([pix.data[0] * 4u8]))
                .count();
        }
        {
            let col_out = img.col_mut(3);
            assert!(col_out.is_none());
        }
        assert_eq!(
            img.into_raw_vec(),
            vec![
                Luma::new([0]),
                Luma::new([3]),
                Luma::new([8]),
                Luma::new([6]),
                Luma::new([12]),
                Luma::new([20]),
                Luma::new([12]),
                Luma::new([21]),
                Luma::new([32]),
            ]
        );
    }

    #[test]
    fn test_cols() {
        let v = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        let img = ImageBuffer2D::<Luma<u8>>::from_raw_vec(3, 3, &v).unwrap();
        for (x, col) in img.cols().enumerate() {
            for (y, pix) in col.into_iter().enumerate() {
                assert_eq!(pix, &Luma::new([3 * y as u8 + x as u8]))
            }
        }
    }

    #[test]
    fn test_cols_mut() {
        let v = [0, 1, 2, 3, 4, 5, 6, 7, 8];
        let mut img = ImageBuffer2D::<Luma<u8>>::from_raw_vec(3, 3, &v).unwrap();
        for (x, col) in img.cols().enumerate() {
            for (y, pix) in col.into_iter().enumerate() {
                assert_eq!(pix, &Luma::new([x as u8 + 3 * y as u8]))
            }
        }
        for (x, col) in img.cols_mut().enumerate() {
            for (y, pix) in col.into_iter().enumerate() {
                *pix = Luma::new([3 * x as u8 + 5 * y as u8]);
            }
        }
        for (x, col) in img.cols().enumerate() {
            for (y, pix) in col.into_iter().enumerate() {
                assert_eq!(pix, &Luma::new([3 * x as u8 + 5 * y as u8]))
            }
        }
    }

    #[test]
    fn test_rect_iter() {
        let v: Vec<Luma<u8>> = (1_u8..16_u8).map(|n| Luma::new([n])).collect();
        let img = ImageBuffer2D::from_vec(5, 3, v).unwrap();
        let subimg1 = img.rect_iter(Rect::new(1, 1, 3, 1));

        fn subimg_vec_eq<'a>(subimg: super::RectIter<'a, Luma<u8>>, v: &[u8]) -> bool {
            v.iter().zip(subimg).all(|(p, l)| *p == l.data[0])
        }

        let subimg1_vec: Vec<u8> = vec![7, 8, 9];

        assert!(subimg_vec_eq(subimg1, &subimg1_vec));
    }

    #[test]
    fn test_translate_rect() {
        let img: ImageBuffer2D<Luma<u8>> = ImageBuffer2D::new(5, 5);
        let r1 = Rect::new(1, 1, 3, 3);
        let r2 = Rect::new(1, 1, 5, 5);
        assert_eq!(img.translate_rect(r1, -2, -2), Some(Rect::new(0, 0, 2, 2)));
        assert_eq!(img.translate_rect(r1, -4, -4), None);
        assert_eq!(img.translate_rect(r1, 2, 2), Some(Rect::new(3, 3, 2, 2)));
        assert_eq!(img.translate_rect(r2, 2, 2), Some(Rect::new(3, 3, 2, 2)));
        assert_eq!(img.translate_rect(r2, 0, 0), Some(Rect::new(1, 1, 4, 4)));
        assert_eq!(img.translate_rect(r1, 4, 4), None);
    }

    #[test]
    fn test_fill_rect() {
        let mut img: ImageBuffer2D<Luma<u8>> = ImageBuffer2D::new(5, 5);
        let r = Rect::new(1, 1, 3, 3);
        img.fill_rect(r, &Luma::<u8>::new([255]));
        for ((x, y), &pixel) in img.enumerate_pixels() {
            if r.contains(x as u32, y as u32) {
                assert_eq!(pixel, Luma::<u8>::new([255]));
            } else {
                assert_eq!(pixel, Luma::<u8>::new([0]));
            }
        }
    }

    #[test]
    fn test_blit_rect() {
        let mut img1 = ImageBuffer2D::<Luma<u8>>::new(64, 64);
        let mut img2 = ImageBuffer2D::<Luma<u8>>::new(64, 64);
        let r = Rect::new(16, 16, 32, 32);
        img2.fill_rect(r, &Luma::<u8>::new([255]));
        assert!(img1.blit_rect(r, r, &img2).is_ok());
        assert_eq!(img1, img2);
    }

    #[test]
    fn test_sub_image() {
        let img = ImageBuffer2D::generate(5, 5, |(x, y)| {
            Luma::from(if y == 0 || y == 4 {
                0u8
            } else {
                (2 * x + 3 * y) as u8
            })
        });
        let sub_img = img.sub_image(Rect::new(1, 1, 3, 3));

        let mut i = 0;
        for ((y, x), p) in sub_img.enumerate_pixels() {
            assert_eq!(&Luma::new([(2 * (x + 1) + 3 * (y + 1)) as u8]), p);
            i += 1;
        }
        assert_eq!(i, 9);
    }

    #[test]
    fn test_sub_image_mut() {
        let mut img = ImageBuffer2D::<Luma<u8>>::new(5, 5);
        {
            let mut sub_img = img.sub_image_mut(Rect::new(1, 1, 3, 3));
            for ((y, x), mut p) in sub_img.enumerate_pixels_mut() {
                p.data[0] = (2 * (x + 1) + 3 * (y + 1)) as u8;
            }
        }

        for ((y, x), p) in img.enumerate_pixels() {
            if x >= 1 && x <= 3 && y >= 1 && y <= 3 {
                assert_eq!(&Luma::new([(2 * x + 3 * y) as u8]), p);
            } else {
                assert_eq!(&Luma::new([0]), p);
            }
        }
    }

    #[test]
    fn test_put_pixel() {
        let mut img = ImageBuffer2D::<Luma<u8>>::new(5, 5);
        {
            for y in 1..4 {
                for x in 1..4 {
                    img.put_pixel(x, y, Luma::new([(2 * x + 3 * y) as u8]));
                }
            }
        }

        for ((y, x), p) in img.enumerate_pixels() {
            if x >= 1 && x <= 3 && y >= 1 && y <= 3 {
                assert_eq!(&Luma::new([(2 * x + 3 * y) as u8]), p);
            } else {
                assert_eq!(&Luma::new([0]), p);
            }
        }
    }

    #[test]
    fn test_get_pixel_mut() {
        let mut img = ImageBuffer2D::<Luma<u8>>::new(5, 5);
        {
            for y in 1..4 {
                for x in 1..4 {
                    *img.get_pixel_mut(x, y) = Luma::new([(2 * x + 3 * y) as u8]);
                }
            }
        }

        for ((y, x), p) in img.enumerate_pixels() {
            if x >= 1 && x <= 3 && y >= 1 && y <= 3 {
                assert_eq!(&Luma::new([(2 * x + 3 * y) as u8]), p);
            } else {
                assert_eq!(&Luma::new([0]), p);
            }
        }
    }

    #[test]
    fn test_index_mut() {
        let mut img = ImageBuffer2D::<Luma<u8>>::new(5, 5);
        {
            for x in 1..4 {
                for y in 1..4 {
                    img[(x, y)][0] = (2 * x + 3 * y) as u8;
                }
            }
        }

        for ((y, x), p) in img.enumerate_pixels() {
            if x >= 1 && x <= 3 && y >= 1 && y <= 3 {
                assert_eq!(&Luma::new([(2 * x + 3 * y) as u8]), p);
            } else {
                assert_eq!(&Luma::new([0]), p);
            }
        }
    }

    #[test]
    fn test_generate() {
        let img = ImageBuffer2D::generate(1280, 720, |(x, y)| Luma::new([5 * x + 13 * y]));
        for ((y, x), pix) in img.enumerate_pixels() {
            assert_eq!(pix, &Luma::new([(5 * x + 13 * y) as u32]));
        }
    }

    #[test]
    #[cfg(feature = "rand_integration")]
    fn test_rand() {
        let img = ImageBuffer2D::<Luma<u8>>::rand(1280, 720, &mut thread_rng());
        let sum = img
            .into_iter()
            .fold(0u32, |acc, p| acc + u32::from(p.data[0]));
        assert!(sum > 100_000_000 && sum < 130_000_000);
    }
}
