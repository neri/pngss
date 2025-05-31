use crate::*;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;
use core::ops::Deref;

pub struct ImageDataOwned {
    pub(crate) info: ImageInfo,
    pub(crate) palette: Vec<RGB888>,
    pub(crate) data: Vec<u8>,
}

pub struct ImageData<'a> {
    pub(crate) info: ImageInfo,
    pub(crate) palette: &'a [RGB888],
    pub(crate) data: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub bit_depth: BitDepth,
    pub color_type: ColorType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorType {
    /// `[y, ...]`
    Grayscale,
    /// `[y, a, ...]`
    GrayscaleAlpha,
    /// `[r, g, b, ...]`
    RGB,
    /// `[r, g, b, a, ...]`
    RGBA,
    /// `[index, ...]`
    Indexed,
}

impl ColorType {
    /// Returns the number of channels for this image type.
    ///
    /// - Grayscale: 1
    /// - GrayscaleAlpha: 2
    /// - RGB: 3
    /// - RGBA: 4
    /// - Indexed: 1
    #[inline]
    pub fn n_channels(&self) -> NumberOfChannnels {
        match self {
            ColorType::Grayscale => NumberOfChannnels::One,
            ColorType::GrayscaleAlpha => NumberOfChannnels::Two,
            ColorType::RGB => NumberOfChannnels::Three,
            ColorType::RGBA => NumberOfChannnels::Four,
            ColorType::Indexed => NumberOfChannnels::One,
        }
    }

    #[inline]
    pub fn to_png_color_type(&self) -> u8 {
        match self {
            Self::Grayscale => 0,
            Self::GrayscaleAlpha => 4,
            Self::RGB => 2,
            Self::RGBA => 6,
            Self::Indexed => 3,
        }
    }

    /// Returns `true` if GrayscaleAlpha or RGBA
    #[inline]
    pub fn has_alpha(&self) -> bool {
        matches!(self, Self::GrayscaleAlpha | Self::RGBA)
    }

    /// Returns `true` if Grayscale or GrayscaleAlpha
    #[inline]
    pub fn is_gray_scale(&self) -> bool {
        matches!(self, Self::Grayscale | Self::GrayscaleAlpha)
    }

    /// Returns `true` if other than Grayscale or GrayscaleAlpha
    #[inline]
    pub fn is_color(&self) -> bool {
        !self.is_gray_scale()
    }

    #[inline]
    pub fn for_each<F, E>(&self, slice: &[u8], palette: &[RGB888], mut kernel: F) -> Result<(), E>
    where
        F: FnMut(color::RGBA8888) -> Result<(), E>,
    {
        for color in self.iter(slice, palette) {
            kernel(color)?;
        }
        Ok(())
    }

    pub fn iter<'a>(
        &self,
        slice: &'a [u8],
        palette: &'a [RGB888],
    ) -> Box<dyn Iterator<Item = color::RGBA8888> + 'a> {
        use color::RGBA8888;
        match self {
            Self::Grayscale => Box::new(slice.iter().map(|&gray| RGBA8888::from_gray(gray))),
            Self::GrayscaleAlpha => Box::new(
                slice
                    .chunks_exact(2)
                    .map(|chunk| RGBA8888::from_gray_alpha(chunk[0], chunk[1])),
            ),
            Self::RGB => Box::new(
                slice
                    .chunks_exact(3)
                    .map(|rgb| RGB888::new(rgb[0], rgb[1], rgb[2]).into()),
            ),
            Self::RGBA => Box::new(
                slice
                    .chunks_exact(4)
                    .map(|rgba| RGBA8888::new(rgba[0], rgba[1], rgba[2], rgba[3])),
            ),
            Self::Indexed => Box::new(
                slice
                    .iter()
                    .map(|&index| palette[index as usize].into_rgba()),
            ),
        }
    }

    pub fn to_rgba_bytes<'a>(&self, input: &'a [u8], palette: &[RGB888]) -> RgbaBytes<'a> {
        match self {
            Self::RGBA => {
                // No conversion needed
                RgbaBytes(Cow::Borrowed(input))
            }
            _ => {
                // Convert to RGBA
                let mut output = Vec::with_capacity(input.len() / self.n_channels().as_usize() * 4);
                for rgba in self.iter(input, palette) {
                    output.push(rgba.r());
                    output.push(rgba.g());
                    output.push(rgba.b());
                    output.push(rgba.a());
                }
                RgbaBytes(Cow::Owned(output))
            }
        }
    }

    pub fn to_rgb_bytes<'a>(&self, input: &'a [u8], palette: &[RGB888]) -> RgbBytes<'a> {
        match self {
            Self::RGB => {
                // No conversion needed
                RgbBytes(Cow::Borrowed(input))
            }
            _ => {
                // Convert to RGB
                let mut output = Vec::with_capacity(input.len() / self.n_channels().as_usize() * 3);
                for rgba in self.iter(input, palette) {
                    output.push(rgba.r());
                    output.push(rgba.g());
                    output.push(rgba.b());
                }
                RgbBytes(Cow::Owned(output))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NumberOfChannnels {
    /// Grayscale or Indexed
    /// `[y, ...]` or `[index, ...]`
    One = 1,
    /// GrayscaleAlpha
    /// `[y, a, ...]`
    Two,
    /// RGB
    /// `[r, g, b, ...]`
    Three,
    /// RGBA
    /// `[r, g, b, a, ...]`
    Four,
}

impl NumberOfChannnels {
    #[inline]
    pub const fn as_usize(self) -> usize {
        self as usize
    }
}

impl ImageDataOwned {
    #[inline]
    pub fn as_ref<'a>(&'a self) -> ImageData<'a> {
        ImageData {
            info: self.info.clone(),
            palette: &self.palette,
            data: &self.data,
        }
    }

    #[inline]
    pub fn info(&self) -> &ImageInfo {
        &self.info
    }

    /// For index color format images, the palette is returned.
    ///
    /// The `raw_data` value will be a byte array representing the index of the palette array, regardless of bit depth.
    #[inline]
    pub fn palette(&self) -> Option<&[RGB888]> {
        (self.info.color_type == ColorType::Indexed).then(|| self.palette.as_ref())
    }

    /// Return image data in raw format.
    ///
    /// If the format is different from your expectations, data conversion is required.
    #[inline]
    pub fn raw_data(&self) -> &[u8] {
        &self.data
    }

    /// Returns an iterator that converts all pixels to RGBA.
    #[inline]
    pub fn all_pixels<'a>(&'a self) -> Box<dyn Iterator<Item = color::RGBA8888> + 'a> {
        self.info.color_type.iter(&self.data, self.palette.as_ref())
    }

    /// Return image data in RGBA format.
    ///
    /// If another format is used, it will be converted.
    #[inline]
    pub fn to_rgba_bytes<'a>(&'a self) -> RgbaBytes<'a> {
        self.info
            .color_type
            .to_rgba_bytes(&self.data, self.palette.as_ref())
    }

    /// Return image data in RGB format.
    ///
    /// If another format is used, it will be converted.
    #[inline]
    pub fn to_rgb_bytes<'a>(&'a self) -> RgbBytes<'a> {
        self.info
            .color_type
            .to_rgb_bytes(&self.data, self.palette.as_ref())
    }
}

impl<'a> ImageData<'a> {
    #[inline]
    pub fn new(
        width: u32,
        height: u32,
        color_type: ColorType,
        palette: &'a [RGB888],
        data: &'a [u8],
    ) -> Self {
        let info = ImageInfo {
            width,
            height,
            bit_depth: BitDepth::Eight,
            color_type,
        };
        Self {
            info,
            palette,
            data,
        }
    }

    #[inline]
    pub fn info(&self) -> &ImageInfo {
        &self.info
    }

    /// For index color format images, the palette is returned.
    ///
    /// The `raw_data` value will be a byte array representing the index of the palette array, regardless of bit depth.
    #[inline]
    pub fn palette(&self) -> Option<&'a [RGB888]> {
        (self.info.color_type == ColorType::Indexed).then(|| self.palette)
    }

    /// Return image data in raw format.
    ///
    /// If the format is different from your expectations, data conversion is required.
    #[inline]
    pub fn raw_data(&self) -> &[u8] {
        &self.data
    }

    /// Returns an iterator that converts all pixels to RGBA.
    #[inline]
    pub fn all_pixels<'b>(&'b self) -> Box<dyn Iterator<Item = color::RGBA8888> + 'b> {
        self.info.color_type.iter(self.data, self.palette)
    }

    /// Return image data in RGBA format.
    ///
    /// If another format is used, it will be converted.
    #[inline]
    pub fn to_rgba_bytes<'b>(&'b self) -> RgbaBytes<'b> {
        self.info.color_type.to_rgba_bytes(self.data, self.palette)
    }

    /// Return image data in RGB format.
    ///
    /// If another format is used, it will be converted.
    #[inline]
    pub fn to_rgb_bytes<'b>(&'b self) -> RgbBytes<'b> {
        self.info.color_type.to_rgb_bytes(self.data, self.palette)
    }
}

/// A byte array stored in the order `R, G, B, A`. It can be `Deref` by `&[u8]`
pub struct RgbaBytes<'a>(Cow<'a, [u8]>);

impl<'a> RgbaBytes<'a> {
    #[inline]
    pub fn into_inner(self) -> Cow<'a, [u8]> {
        self.0
    }

    #[inline]
    pub fn into_owned(self) -> Vec<u8> {
        self.0.into_owned()
    }
}

impl Deref for RgbaBytes<'_> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

/// A byte array stored in the order `R, G, B`. It can be `Deref` by `&[u8]`
pub struct RgbBytes<'a>(Cow<'a, [u8]>);

impl<'a> RgbBytes<'a> {
    #[inline]
    pub fn into_inner(self) -> Cow<'a, [u8]> {
        self.0
    }

    #[inline]
    pub fn into_owned(self) -> Vec<u8> {
        self.0.into_owned()
    }
}

impl Deref for RgbBytes<'_> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BitDepth {
    One = 1,
    Two = 2,
    Four = 4,
    Eight = 8,
}

impl BitDepth {
    pub fn new(val: u8) -> Option<Self> {
        match val {
            1 => Some(Self::One),
            2 => Some(Self::Two),
            4 => Some(Self::Four),
            8 => Some(Self::Eight),
            _ => None,
        }
    }

    #[inline]
    pub fn bits_per_pixel(&self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Four => 4,
            Self::Eight => 8,
        }
    }
}

impl fmt::Debug for BitDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BitDepth({})", self.bits_per_pixel())
    }
}
