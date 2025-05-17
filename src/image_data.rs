use crate::*;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ops::{Deref, DerefMut};

pub struct ImageData {
    pub(crate) info: ImageInfo,
    pub(crate) palette: Vec<RGB888>,
    pub(crate) data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub bit_depth: BitDepth,
    pub image_type: ImageType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageType {
    Grayscale,
    GrayscaleAlpha,
    RGB,
    RGBA,
    Indexed,
}

impl ImageType {
    #[inline]
    pub fn n_channels(&self) -> usize {
        match self {
            ImageType::Grayscale => 1,
            ImageType::GrayscaleAlpha => 2,
            ImageType::RGB => 3,
            ImageType::RGBA => 4,
            ImageType::Indexed => 1,
        }
    }

    #[inline]
    pub fn has_alpha(&self) -> bool {
        matches!(self, Self::GrayscaleAlpha | Self::RGBA)
    }

    #[inline]
    pub fn is_gray_scale(&self) -> bool {
        matches!(self, Self::Grayscale | Self::GrayscaleAlpha)
    }

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
                    .map(|chunk| RGBA8888::from_rgb(chunk[0], chunk[1], chunk[2])),
            ),
            Self::RGBA => Box::new(
                slice
                    .chunks_exact(4)
                    .map(|chunk| RGBA8888::from_rgba(chunk[0], chunk[1], chunk[2], chunk[3])),
            ),
            Self::Indexed => Box::new(
                slice
                    .iter()
                    .map(|index| palette[*index as usize].into_rgba()),
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
                let mut output = Vec::with_capacity(input.len() / self.n_channels() * 4);
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
                let mut output = Vec::with_capacity(input.len() / self.n_channels() * 3);
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

impl ImageData {
    #[inline]
    pub fn info(&self) -> &ImageInfo {
        &self.info
    }

    /// For index color format images, the palette is returned.
    ///
    /// The `raw_data` value represents the index of the palette array, regardless of bit depth.
    #[inline]
    pub fn palette(&self) -> Option<&[RGB888]> {
        if self.info.image_type == ImageType::Indexed {
            Some(&self.palette)
        } else {
            None
        }
    }

    /// Return image data in raw format.
    ///
    /// If the format is different from your expectations, data conversion is required.
    #[inline]
    pub fn raw_data(&self) -> &[u8] {
        &self.data
    }

    /// Return image data in RGBA format.
    ///
    /// If another format is used, it will be converted.
    #[inline]
    pub fn to_rgba_bytes<'a>(&'a self) -> RgbaBytes<'a> {
        self.info
            .image_type
            .to_rgba_bytes(self.data.as_slice(), &self.palette)
    }

    /// Return image data in RGB format.
    ///
    /// If another format is used, it will be converted.
    #[inline]
    pub fn to_rgb_bytes<'a>(&'a self) -> RgbBytes<'a> {
        self.info
            .image_type
            .to_rgb_bytes(self.data.as_slice(), &self.palette)
    }
}

pub struct RgbaBytes<'a>(Cow<'a, [u8]>);

impl Deref for RgbaBytes<'_> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl DerefMut for RgbaBytes<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.to_mut()
    }
}

pub struct RgbBytes<'a>(Cow<'a, [u8]>);

impl Deref for RgbBytes<'_> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl DerefMut for RgbBytes<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.to_mut()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BitDepth {
    Bpp1 = 1,
    Bpp2 = 2,
    Bpp4 = 4,
    Bpp8 = 8,
}

impl BitDepth {
    pub fn new(val: u8) -> Option<Self> {
        match val {
            1 => Some(Self::Bpp1),
            2 => Some(Self::Bpp2),
            4 => Some(Self::Bpp4),
            8 => Some(Self::Bpp8),
            _ => None,
        }
    }

    #[inline]
    pub fn bits_per_pixel(&self) -> u8 {
        match self {
            Self::Bpp1 => 1,
            Self::Bpp2 => 2,
            Self::Bpp4 => 4,
            Self::Bpp8 => 8,
        }
    }
}
