//! A subset implementation of PNG Encoder

use super::*;
use alloc::collections::BTreeMap;
use compress::entropy::entropy_of_blocks;

/// Default PNG encoder using the default deflate implementation.
pub type PngEncoder = CustomPngEncoder<DefaultDeflateEncoder>;

/// Interface to implement deflate decompression function
pub trait DeflateEncoder {
    fn deflate(input: &[u8], level: CompressionLevel) -> Result<Vec<u8>, EncodeError>;
}

pub struct CustomPngEncoder<DE: DeflateEncoder> {
    _phantom: core::marker::PhantomData<DE>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompressionLevel {
    /// Encodes as fast as possible. Format conversion is also minimized.
    Fast,
    /// Default compression level, usually balanced between speed and size.
    Default,
    /// Best compression level, usually slower but results in smaller size.
    Best,
}

/// Wrapper for the deflate function used internally.
///
/// It may be replaced by another implementation in the future.
pub struct DefaultDeflateEncoder;

impl DeflateEncoder for DefaultDeflateEncoder {
    #[inline(always)]
    fn deflate(input: &[u8], level: CompressionLevel) -> Result<Vec<u8>, EncodeError> {
        deflate::deflate_zlib(
            input,
            match level {
                CompressionLevel::Fast => deflate::CompressionLevel::Fastest,
                CompressionLevel::Default => deflate::CompressionLevel::Default,
                CompressionLevel::Best => deflate::CompressionLevel::Best,
            },
            None,
        )
        .map_err(|_| EncodeError::InvalidInput)
    }
}

impl<DE: DeflateEncoder> CustomPngEncoder<DE> {
    /// Encodes the image data into PNG format.
    ///
    /// This function will attempt to generate an indexed PNG if the image is suitable for it.
    pub fn encode(image: &ImageData, level: CompressionLevel) -> Result<Vec<u8>, EncodeError> {
        if level == CompressionLevel::Fast {
            return Self::encode_as_is(image, level);
        }
        let info = image.info();
        if info.color_type == ColorType::Indexed {
            let palette = image.palette().ok_or(EncodeError::InvalidInput)?;
            return Self::encode_indexed(info.width, info.height, &image.data, palette, level);
        }
        if info.width == 0
            || info.height == 0
            || image.data.len()
                < info.width as usize * info.height as usize * info.color_type.n_channels() as usize
        {
            return Err(EncodeError::InvalidInput);
        }

        if let Some((palette, is_gray)) = attempt_to_generate_palette(&image) {
            if palette.len() <= 16 || !is_gray {
                let mut new_data = Vec::with_capacity(info.width as usize * info.height as usize);
                for rgba in image.all_pixels() {
                    let rgb = RGB888::from_rgba(rgba);
                    let index = palette.binary_search(&rgb).unwrap_or_else(|e| e);
                    new_data.push(index as u8);
                }
                let indexed =
                    Self::encode_indexed(info.width, info.height, &new_data, &palette, level)?;
                let as_is = Self::encode_as_is(image, level)?;
                if indexed.len() < as_is.len() {
                    return Ok(indexed);
                } else {
                    return Ok(as_is);
                }
            }
        }

        Self::encode_as_is(image, level)
    }

    /// Encode to PNG as input format.
    pub fn encode_as_is(
        image: &ImageData,
        level: CompressionLevel,
    ) -> Result<Vec<u8>, EncodeError> {
        let info = image.info();
        if info.color_type == ColorType::Indexed {
            let palette = image.palette().ok_or(EncodeError::InvalidInput)?;
            return Self::encode_indexed(info.width, info.height, &image.data, palette, level);
        }
        if info.width == 0
            || info.height == 0
            || image.data.len()
                < info.width as usize * info.height as usize * info.color_type.n_channels() as usize
        {
            return Err(EncodeError::InvalidInput);
        }

        Self::generate_png(
            info.width,
            info.height,
            &Self::generate_idat(
                info.width as usize * info.color_type.n_channels() as usize,
                info.height,
                image.data,
                info.color_type.n_channels(),
                level,
                false,
            )?,
            None,
            BitDepth::Eight,
            info.color_type,
        )
    }

    /// Encodes to PNG with index color.
    pub fn encode_indexed(
        width: u32,
        height: u32,
        data: &[u8],
        palette: &[RGB888],
        level: CompressionLevel,
    ) -> Result<Vec<u8>, EncodeError> {
        if width == 0 || height == 0 || data.len() < width as usize * height as usize {
            return Err(EncodeError::InvalidInput);
        }
        let max_data = data.iter().copied().max().unwrap_or(0);
        if max_data as usize >= palette.len() {
            return Err(EncodeError::InvalidInput);
        }
        let bits = index_color_bits(max_data);
        let (stride, data) = match bits {
            BitDepth::One => {
                let mut fixed = Vec::with_capacity((width as usize + 15) / 8 * height as usize);
                for line in data.chunks_exact(width as usize) {
                    for chunk in line.chunks(8) {
                        let mut acc = 0;
                        let mut bit = 0x80;
                        for &pixel in chunk {
                            if pixel != 0 {
                                acc |= bit;
                            }
                            bit >>= 1;
                        }
                        fixed.push(acc);
                    }
                }
                ((width as usize + 7) / 8, Cow::Owned(fixed))
            }
            BitDepth::Two => {
                let mut fixed = Vec::with_capacity((width as usize + 7) / 4 * height as usize);
                for line in data.chunks_exact(width as usize) {
                    for chunk in line.chunks(4) {
                        let mut acc = 0;
                        for (index, pixel) in chunk.iter().enumerate() {
                            acc |= pixel << ((3 - index) * 2);
                        }
                        fixed.push(acc);
                    }
                }
                ((width as usize + 3) / 4, Cow::Owned(fixed))
            }
            BitDepth::Four => {
                let mut fixed = Vec::with_capacity((width as usize + 1) / 2 * height as usize);
                for line in data.chunks_exact(width as usize) {
                    for chunk in line.chunks(2) {
                        let mut acc = 0;
                        for (index, pixel) in chunk.iter().enumerate() {
                            acc |= pixel << ((1 - index) * 4);
                        }
                        fixed.push(acc);
                    }
                }
                ((width as usize + 1) / 2, Cow::Owned(fixed))
            }
            BitDepth::Eight => (width as usize, Cow::Borrowed(data)),
        };

        let idat = if level == CompressionLevel::Fast {
            Self::generate_idat(stride, height, &data, NumberOfChannnels::One, level, true)?
        } else {
            let idat1 =
                Self::generate_idat(stride, height, &data, NumberOfChannnels::One, level, true)?;
            let idat2 =
                Self::generate_idat(stride, height, &data, NumberOfChannnels::One, level, false)?;
            if idat1.len() < idat2.len() {
                idat1
            } else {
                idat2
            }
        };

        Self::generate_png(
            width,
            height,
            &idat,
            Some(palette),
            bits,
            ColorType::Indexed,
        )
    }

    /// Apply filters and compression to image data to generate the source of IDAT.
    fn generate_idat(
        stride: usize,
        height: u32,
        data: &[u8],
        n_channels: NumberOfChannnels,
        level: CompressionLevel,
        is_filter_none: bool,
    ) -> Result<Vec<u8>, EncodeError> {
        let mut new_data = Vec::with_capacity((1 + stride) * height as usize);
        if is_filter_none || level == CompressionLevel::Fast {
            for line in data.chunks_exact(stride) {
                new_data.push(FilterType::None as u8);
                new_data.extend_from_slice(line);
            }
        } else {
            let mut prev_line = None;
            for current_line in data.chunks_exact(stride) {
                let (filter, new_line) =
                    Self::process_scanline(current_line, &prev_line, n_channels);
                new_data.push(filter as u8);
                new_data.extend_from_slice(&new_line);
                prev_line = Some(current_line);
            }
        }
        DE::deflate(&new_data, level)
    }

    /// Processes a single scanline of image data with various filters and selects the best one based on entropy.
    fn process_scanline<'a>(
        this_line: &'a [u8],
        prev_line: &Option<&[u8]>,
        n_channels: NumberOfChannnels,
    ) -> (FilterType, Cow<'a, [u8]>) {
        // Filter None
        // Filt(x) = Orig(x)
        let mut selected_line = (
            entropy_of_blocks(&[&[FilterType::None as u8], this_line]),
            FilterType::None,
            Cow::Borrowed(this_line),
        );

        {
            // Filter Sub
            // Filt(x) = Orig(x) - Orig(a)
            let mut left_pixel = [0; 4];
            let mut new_data = Vec::with_capacity(this_line.len());
            for chunk in this_line.chunks_exact(n_channels.as_usize()) {
                for (left, &current) in left_pixel.iter_mut().zip(chunk.iter()) {
                    let filt = current.wrapping_sub(*left);
                    new_data.push(filt);
                    *left = current;
                }
            }
            let entropy = entropy_of_blocks(&[&[FilterType::Sub as u8], &new_data]);
            if entropy < selected_line.0 {
                selected_line = (entropy, FilterType::Sub, Cow::Owned(new_data));
            }
        }

        if let Some(prev_line) = prev_line {
            {
                // Filter Up
                // Filt(x) = Orig(x) - Orig(b)
                let mut new_data = Vec::with_capacity(this_line.len());
                for (current, &above) in this_line.iter().zip(prev_line.iter()) {
                    let filt = current.wrapping_sub(above);
                    new_data.push(filt);
                }
                let entropy = entropy_of_blocks(&[&[FilterType::Up as u8], &new_data]);
                if entropy < selected_line.0 {
                    selected_line = (entropy, FilterType::Up, Cow::Owned(new_data));
                }
            }

            {
                // Filter Average
                // Filt(x) = Orig(x) - floor((Orig(a) + Orig(b)) / 2)
                let mut new_data = Vec::with_capacity(this_line.len());
                let mut left_pixel = [0; 4];
                for (current, prev) in this_line
                    .chunks_exact(n_channels.as_usize())
                    .zip(prev_line.chunks_exact(n_channels.as_usize()))
                {
                    for (left, (&current, &above)) in
                        left_pixel.iter_mut().zip(current.iter().zip(prev.iter()))
                    {
                        let filt = current.wrapping_sub(average(*left, above));
                        new_data.push(filt);
                        *left = current;
                    }
                }
                let entropy = entropy_of_blocks(&[&[FilterType::Average as u8], &new_data]);
                if entropy < selected_line.0 {
                    selected_line = (entropy, FilterType::Average, Cow::Owned(new_data));
                }
            }

            {
                // Filter Paeth
                // Filt(x) = Orig(x) - PaethPredictor(Orig(a), Orig(b), Orig(c))
                let mut new_data = Vec::with_capacity(this_line.len());
                let mut left_pixel = [0; 4];
                let mut upper_left_pixel = [0; 4];
                for (current, prev) in this_line
                    .chunks_exact(n_channels.as_usize())
                    .zip(prev_line.chunks_exact(n_channels.as_usize()))
                {
                    for ((left, upper_left), (&current, &above)) in left_pixel
                        .iter_mut()
                        .zip(upper_left_pixel.iter_mut())
                        .zip(current.iter().zip(prev.iter()))
                    {
                        let filt = current.wrapping_sub(paeth(*left, above, *upper_left));
                        new_data.push(filt);
                        *left = current;
                        *upper_left = above;
                    }
                }
                let entropy = entropy_of_blocks(&[&[FilterType::Paeth as u8], &new_data]);
                if entropy < selected_line.0 {
                    selected_line = (entropy, FilterType::Paeth, Cow::Owned(new_data));
                }
            }
        }

        (selected_line.1, selected_line.2)
    }

    fn generate_png(
        width: u32,
        height: u32,
        data: &[u8],
        palette: Option<&[RGB888]>,
        bit_depth: BitDepth,
        color_type: ColorType,
    ) -> Result<Vec<u8>, EncodeError> {
        if width == 0 || height == 0 {
            return Err(EncodeError::InvalidInput);
        }

        let mut output = Vec::new();
        output.extend_from_slice(PNG_SIGNATURE);

        let mut ihdr = [0; IHDR_SIZE];
        ihdr[0..4].copy_from_slice(&width.to_be_bytes());
        ihdr[4..8].copy_from_slice(&height.to_be_bytes());
        ihdr[8] = bit_depth as u8;
        ihdr[9] = color_type.to_png_color_type();
        ihdr[10] = 0; // Compression method
        ihdr[11] = 0; // Filter method
        ihdr[12] = 0; // Interlace method
        PngChunk::new(FourCC::IHDR, &ihdr).write_to(&mut output);

        if let Some(palette) = palette {
            let palette = palette
                .iter()
                .map(|v| [v.r, v.g, v.b])
                .flatten()
                .collect::<Vec<_>>();
            PngChunk::new(FourCC::PLTE, &palette).write_to(&mut output);
        }

        PngChunk::new(FourCC::IDAT, data).write_to(&mut output);

        PngChunk::new(FourCC::IEND, &[]).write_to(&mut output);

        Ok(output)
    }
}

/// Attempts to generate a palette from the image data.
///
/// Returns a tuple containing the palette and a boolean indicating if the palette is grayscale.
pub fn attempt_to_generate_palette(image: &ImageData) -> Option<(Vec<RGB888>, bool)> {
    let mut palette = BTreeMap::new();
    let mut is_gray = true;
    for rgba in image.all_pixels() {
        if rgba.a() != 0xFF {
            return None;
        }
        let rgb = RGB888::from_rgba(rgba);
        is_gray &= rgb.is_gray();
        palette.entry(rgb).or_insert(1);
        if palette.keys().len() > 256 {
            return None;
        }
    }
    let mut palette = palette.into_iter().map(|v| v.0).collect::<Vec<_>>();
    palette.sort();

    Some((palette, is_gray))
}

#[inline]
fn index_color_bits(max_data: u8) -> BitDepth {
    if max_data < 2 {
        BitDepth::One
    } else if max_data < 4 {
        BitDepth::Two
    } else if max_data < 16 {
        BitDepth::Four
    } else {
        BitDepth::Eight
    }
}
