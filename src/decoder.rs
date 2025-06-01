//! A subset implementation of PNG Decoder

use super::*;
use core::{
    marker::PhantomData,
    sync::atomic::{Ordering, compiler_fence},
};

/// Default PNG decoder using the default deflate decoder.
pub type PngDecoder<'a> = CustomPngDecoder<'a, DefaultDeflateDecoder>;

/// Interface to implement the inflate (deflate decompression) function
pub trait DeflateDecoder {
    fn inflate(input: &[u8], size: usize) -> Result<Vec<u8>, DecodeError>;
}

pub struct CustomPngDecoder<'a, DD: DeflateDecoder> {
    slice: &'a [u8],
    info: ImageInfo,
    bit_depth: BitDepth,
    _phantom: PhantomData<DD>,
}

/// Wrapper for the inflate function used internally.
///
/// It may be replaced by another implementation in the future.
pub struct DefaultDeflateDecoder;

impl DeflateDecoder for DefaultDeflateDecoder {
    #[inline(always)]
    fn inflate(input: &[u8], size: usize) -> Result<Vec<u8>, DecodeError> {
        deflate::inflate(input, size).map_err(|_| DecodeError::InvalidData)
    }
}

impl<'a, DD: DeflateDecoder> CustomPngDecoder<'a, DD> {
    /// Generates a PNG decoder from the specified slice.
    ///
    /// Returns an error if the signature is invalid, if the IHDR chunk does not exist, or if it has unsupported information.
    pub fn new(input: &'a [u8]) -> Result<Self, DecodeError> {
        let (signature, next) = input.split_at_checked(8).ok_or(DecodeError::BadSignature)?;
        if signature != PNG_SIGNATURE {
            // PNG signature must be the first 8 bytes.
            return Err(DecodeError::BadSignature);
        }

        let (ihdr, next) = next
            .split_at_checked(12 + IHDR_SIZE)
            .ok_or(DecodeError::InvalidData)?;
        let mut ihdr = PngChunksInner { slice: ihdr };
        let ihdr = ihdr.next_chunk()?;
        if ihdr.chunk_type() != FourCC::IHDR || ihdr.len() != IHDR_SIZE {
            // IHDR chunk must be the first chunk and must have a size of 13 bytes.
            return Err(DecodeError::InvalidData);
        }

        let width = Be32(ihdr.data()[0..4].try_into().unwrap()).as_u32();
        let height = Be32(ihdr.data()[4..8].try_into().unwrap()).as_u32();
        if width == 0 || height == 0 {
            // Zero is an invalid value.
            return Err(DecodeError::InvalidData);
        }
        if cfg!(target_pointer_width = "32") && (width.saturating_mul(height) > 0x1000_0000) {
            // TODO: maybe overflow
            return Err(DecodeError::UnsupportedFormat);
        }

        let Some(bit_depth) = BitDepth::new(ihdr.data()[8]) else {
            return Err(DecodeError::UnsupportedFormat);
        };
        let color_type = ihdr.data()[9];
        let color_type = match (color_type, bit_depth) {
            (0, BitDepth::Eight) => ColorType::Grayscale,
            (2, BitDepth::Eight) => ColorType::RGB,
            (3, BitDepth::One)
            | (3, BitDepth::Two)
            | (3, BitDepth::Four)
            | (3, BitDepth::Eight) => ColorType::Indexed,
            (4, BitDepth::Eight) => ColorType::GrayscaleAlpha,
            (6, BitDepth::Eight) => ColorType::RGBA,
            // Unsupported color types
            _ => return Err(DecodeError::UnsupportedFormat),
        };

        let compression_method = ihdr.data()[10];
        let filter_method = ihdr.data()[11];
        let interlace_method = ihdr.data()[12];
        if compression_method != 0 || filter_method != 0 || interlace_method != 0 {
            // Only compression method 0, filter method 0, and interlace method 0 are supported.
            return Err(DecodeError::UnsupportedFormat);
        }

        let info = ImageInfo {
            width,
            height,
            color_type,
        };

        Ok(Self {
            slice: next,
            info,
            bit_depth,
            _phantom: PhantomData,
        })
    }

    #[inline]
    fn chunks_unchecked(&self) -> PngChunksInner<'a> {
        PngChunksInner { slice: self.slice }
    }

    /// Returns an iterator over the chunks in the PNG file.
    #[inline]
    pub fn chunks(&self) -> Result<PngChunks<'a>, DecodeError> {
        let mut test = self.chunks_unchecked();
        let mut idat_count = 0;
        let mut idat_size = 0;
        loop {
            let chunk = test.next_chunk()?;
            if chunk.chunk_type() == FourCC::IDAT {
                idat_count += 1;
                idat_size += chunk.len();
            }
            if chunk.is_valid_iend() {
                break;
            }
        }

        Ok(PngChunks {
            idat_count,
            idat_size,
            inner: self.chunks_unchecked(),
        })
    }

    /// Returns the image information.
    #[inline]
    pub fn info(&self) -> &ImageInfo {
        &self.info
    }

    /// Returns the bit depth of the image.
    #[inline]
    pub fn bit_depth(&self) -> BitDepth {
        self.bit_depth
    }

    /// Returns the size of the buffer required to decompress IDAT chunks.
    ///
    /// Equals to `(1 + width * n_channels) * height`
    #[inline]
    pub fn decoded_buffer_size(&self) -> usize {
        (1 + self.info.width as usize * self.info.color_type.n_channels() as usize)
            * self.info.height as usize
    }

    /// Decodes PNG images and returns image data.
    pub fn decode(&self) -> Result<ImageDataOwned, DecodeError> {
        let mut chunks = self.chunks()?;
        let mut palette = Option::<Vec<RGB888>>::None;

        // Read chunks before IDAT
        loop {
            let chunk = chunks.peek()?;
            match chunk.chunk_type() {
                FourCC::IDAT => break,
                FourCC::PLTE => {
                    if chunk.len() % 3 != 0 || palette.is_some() {
                        return Err(DecodeError::InvalidData);
                    }
                    palette = Some(
                        chunk
                            .data()
                            .chunks_exact(3)
                            .map(|v| RGB888::new(v[0], v[1], v[2]))
                            .collect(),
                    );
                }
                four_cc => {
                    if four_cc.is_critical() {
                        return Err(DecodeError::UnsupportedFormat);
                    }
                }
            }
            chunks.next().ok_or(DecodeError::InvalidData)?;
        }

        // Get IDAT chunks
        let data = chunks.get_idat_chunks(true)?;

        // Decompress the IDAT data
        let buffer = DD::inflate(&data, self.decoded_buffer_size())?;

        // Apply filters
        let mut reconstructed = self.apply_filter(buffer)?;

        // fix bit depth less than 8
        if self.bit_depth < BitDepth::Eight {
            let mut fixed =
                Vec::with_capacity(self.info.width as usize * self.info.height as usize);
            match self.bit_depth {
                BitDepth::One => {
                    let mut iter = reconstructed.iter();
                    let iter = &mut iter;
                    let w8 = self.info.width as usize / 8;
                    let w8r = self.info.width as usize & 7;
                    for _y in 0..self.info.height as usize {
                        for &byte in iter.take(w8) {
                            for i in (0..8).rev() {
                                fixed.push((byte >> i) & 0x01);
                            }
                        }
                        if w8r > 0 {
                            let byte = iter.next().unwrap();
                            for i in (0..w8r).rev() {
                                fixed.push((byte >> i) & 0x01);
                            }
                        }
                    }
                }
                BitDepth::Two => {
                    let mut iter = reconstructed.iter();
                    let iter = &mut iter;
                    let w4 = self.info.width as usize / 4;
                    let w4r = self.info.width as usize & 3;
                    for _y in 0..self.info.height as usize {
                        for &byte in iter.take(w4) {
                            for i in (0..4).rev() {
                                fixed.push((byte >> (i * 2)) & 0x03);
                            }
                        }
                        if w4r > 0 {
                            let byte = iter.next().unwrap();
                            for i in (0..w4r).rev() {
                                fixed.push((byte >> (i * 2)) & 0x03);
                            }
                        }
                    }
                }
                BitDepth::Four => {
                    let mut iter = reconstructed.iter();
                    let iter = &mut iter;
                    let w2 = self.info.width as usize / 2;
                    let w2r = self.info.width as usize & 1;
                    for _y in 0..self.info.height as usize {
                        for &byte in iter.take(w2) {
                            for i in (0..2).rev() {
                                fixed.push((byte >> (i * 4)) & 0x0f);
                            }
                        }
                        if w2r > 0 {
                            let byte = iter.next().unwrap();
                            for i in (0..w2r).rev() {
                                fixed.push((byte >> (i * 4)) & 0x0f);
                            }
                        }
                    }
                }
                BitDepth::Eight => {
                    unreachable!()
                }
            }
            reconstructed = fixed;
        }

        // pallete check
        if self.info.color_type == ColorType::Indexed {
            let Some(palette) = palette.as_ref() else {
                return Err(DecodeError::InvalidData);
            };
            let max_index = reconstructed.iter().copied().max().unwrap() as usize;
            if palette.len() > 256 || max_index >= palette.len() {
                return Err(DecodeError::InvalidData);
            }
        }

        // return the image data
        Ok(ImageDataOwned {
            info: self.info,
            palette: palette.unwrap_or_default(),
            data: reconstructed,
        })
    }

    #[inline(always)]
    fn apply_filter(&self, mut buffer: Vec<u8>) -> Result<Vec<u8>, DecodeError> {
        let width = self.info.width as usize;
        let stride = if self.bit_depth > BitDepth::Eight {
            width as usize * self.info.color_type.n_channels() as usize
        } else {
            (width as usize * self.info.color_type.n_channels() as usize * self.bit_depth as usize
                + 7)
                / 8
        };

        match self.info.color_type.n_channels() {
            NumberOfChannnels::One => {
                let mut dest = 0;
                let mut src = 0;
                let mut prev_line = Option::<usize>::None;
                for _y in 0..self.info.height {
                    let filter_type = buffer[src];
                    let filter_type =
                        FilterType::new(filter_type).ok_or(DecodeError::InvalidData)?;
                    src += 1;
                    let this_line = dest;
                    match filter_type {
                        // Recon(x) = Filt(x)
                        FilterType::None => {
                            for _x in 0..stride {
                                buffer[dest] = buffer[src];
                                dest += 1;
                                src += 1;
                            }
                        }
                        // Recon(x) = Filt(x) + Recon(a)
                        FilterType::Sub => {
                            let mut left_pixel = 0;
                            for _x in 0..width {
                                let y = buffer[src].wrapping_add(left_pixel);
                                buffer[dest] = y;
                                left_pixel = y;
                                dest += 1;
                                src += 1;
                            }
                        }
                        // Recon(x) = Filt(x) + Recon(b)
                        FilterType::Up => {
                            if let Some(prev_line) = prev_line {
                                let mut src2 = prev_line;
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src].wrapping_add(buffer[src2]);
                                    dest += 1;
                                    src += 1;
                                    src2 += 1;
                                }
                            } else {
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src];
                                    dest += 1;
                                    src += 1;
                                }
                            }
                        }
                        // Recon(x) = Filt(x) + floor((Recon(a) + Recon(b)) / 2)
                        FilterType::Average => {
                            if let Some(prev_line) = prev_line {
                                let mut left_pixel = 0;
                                let mut src2 = prev_line;
                                for _x in 0..width {
                                    let y =
                                        buffer[src].wrapping_add(average(left_pixel, buffer[src2]));
                                    compiler_fence(Ordering::SeqCst);
                                    buffer[dest] = y;
                                    left_pixel = y;
                                    dest += 1;
                                    src += 1;
                                    src2 += 1;
                                }
                            } else {
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src];
                                    dest += 1;
                                    src += 1;
                                }
                            }
                        }
                        // Recon(x) = Filt(x) + PaethPredictor(Recon(a), Recon(b), Recon(c))
                        FilterType::Paeth => {
                            if let Some(prev_line) = prev_line {
                                let mut left_pixel = 0;
                                let mut upper_left = 0;
                                let mut src2 = prev_line;
                                for _x in 0..width {
                                    let above_y = buffer[src2];
                                    let y = buffer[src]
                                        .wrapping_add(paeth(left_pixel, above_y, upper_left));
                                    compiler_fence(Ordering::SeqCst);
                                    buffer[dest] = y;
                                    left_pixel = y;
                                    upper_left = above_y;
                                    dest += 1;
                                    src += 1;
                                    src2 += 1;
                                }
                            } else {
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src];
                                    dest += 1;
                                    src += 1;
                                }
                            }
                        }
                    }
                    prev_line = Some(this_line);
                }
            }
            NumberOfChannnels::Two => {
                let mut dest = 0;
                let mut src = 0;
                let mut prev_line = Option::<usize>::None;
                for _y in 0..self.info.height {
                    let filter_type = buffer[src];
                    let filter_type =
                        FilterType::new(filter_type).ok_or(DecodeError::InvalidData)?;
                    src += 1;
                    let this_line = dest;
                    match filter_type {
                        // Recon(x) = Filt(x)
                        FilterType::None => {
                            for _x in 0..stride {
                                buffer[dest] = buffer[src];
                                dest += 1;
                                src += 1;
                            }
                        }
                        // Recon(x) = Filt(x) + Recon(a)
                        FilterType::Sub => {
                            let mut left_pixel = [0; 4];
                            for _x in 0..width {
                                let y = buffer[src].wrapping_add(left_pixel[0]);
                                let a = buffer[src + 1].wrapping_add(left_pixel[1]);
                                buffer[dest] = y;
                                buffer[dest + 1] = a;
                                left_pixel[0] = y;
                                left_pixel[1] = a;
                                dest += 2;
                                src += 2;
                            }
                        }
                        // Recon(x) = Filt(x) + Recon(b)
                        FilterType::Up => {
                            if let Some(prev_line) = prev_line {
                                let mut src2 = prev_line;
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src].wrapping_add(buffer[src2]);
                                    dest += 1;
                                    src += 1;
                                    src2 += 1;
                                }
                            } else {
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src];
                                    dest += 1;
                                    src += 1;
                                }
                            }
                        }
                        // Recon(x) = Filt(x) + floor((Recon(a) + Recon(b)) / 2)
                        FilterType::Average => {
                            if let Some(prev_line) = prev_line {
                                let mut left_pixel = [0; 4];
                                let mut src2 = prev_line;
                                for _x in 0..width {
                                    let y = buffer[src]
                                        .wrapping_add(average(left_pixel[0], buffer[src2]));
                                    let a = buffer[src + 1]
                                        .wrapping_add(average(left_pixel[1], buffer[src2 + 1]));
                                    compiler_fence(Ordering::SeqCst);
                                    buffer[dest] = y;
                                    buffer[dest + 1] = a;
                                    left_pixel[0] = y;
                                    left_pixel[1] = a;
                                    dest += 2;
                                    src += 2;
                                    src2 += 2;
                                }
                            } else {
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src];
                                    dest += 1;
                                    src += 1;
                                }
                            }
                        }
                        // Recon(x) = Filt(x) + PaethPredictor(Recon(a), Recon(b), Recon(c))
                        FilterType::Paeth => {
                            if let Some(prev_line) = prev_line {
                                let mut left_pixel = [0; 4];
                                let mut upper_left = [0; 4];
                                let mut src2 = prev_line;
                                for _x in 0..width {
                                    let (above_y, above_a) = (buffer[src2], buffer[src2 + 1]);
                                    let y = buffer[src].wrapping_add(paeth(
                                        left_pixel[0],
                                        above_y,
                                        upper_left[0],
                                    ));
                                    let a = buffer[src + 1].wrapping_add(paeth(
                                        left_pixel[1],
                                        above_a,
                                        upper_left[1],
                                    ));
                                    compiler_fence(Ordering::SeqCst);
                                    buffer[dest] = y;
                                    buffer[dest + 1] = a;
                                    left_pixel[0] = y;
                                    left_pixel[1] = a;
                                    upper_left[0] = above_y;
                                    upper_left[1] = above_a;
                                    dest += 2;
                                    src += 2;
                                    src2 += 2;
                                }
                            } else {
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src];
                                    dest += 1;
                                    src += 1;
                                }
                            }
                        }
                    }
                    prev_line = Some(this_line);
                }
            }
            NumberOfChannnels::Three => {
                let mut dest = 0;
                let mut src = 0;
                let mut prev_line = Option::<usize>::None;
                for _y in 0..self.info.height {
                    let filter_type = buffer[src];
                    let filter_type =
                        FilterType::new(filter_type).ok_or(DecodeError::InvalidData)?;
                    src += 1;
                    let this_line = dest;
                    match filter_type {
                        // Recon(x) = Filt(x)
                        FilterType::None => {
                            for _x in 0..stride {
                                buffer[dest] = buffer[src];
                                dest += 1;
                                src += 1;
                            }
                        }
                        // Recon(x) = Filt(x) + Recon(a)
                        FilterType::Sub => {
                            let mut left_pixel = [0; 4];
                            for _x in 0..width {
                                let r = buffer[src].wrapping_add(left_pixel[0]);
                                let g = buffer[src + 1].wrapping_add(left_pixel[1]);
                                let b = buffer[src + 2].wrapping_add(left_pixel[2]);
                                buffer[dest] = r;
                                buffer[dest + 1] = g;
                                buffer[dest + 2] = b;
                                left_pixel[0] = r;
                                left_pixel[1] = g;
                                left_pixel[2] = b;
                                dest += 3;
                                src += 3;
                            }
                        }
                        // Recon(x) = Filt(x) + Recon(b)
                        FilterType::Up => {
                            if let Some(prev_line) = prev_line {
                                let mut src2 = prev_line;
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src].wrapping_add(buffer[src2]);
                                    dest += 1;
                                    src += 1;
                                    src2 += 1;
                                }
                            } else {
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src];
                                    dest += 1;
                                    src += 1;
                                }
                            }
                        }
                        // Recon(x) = Filt(x) + floor((Recon(a) + Recon(b)) / 2)
                        FilterType::Average => {
                            if let Some(prev_line) = prev_line {
                                let mut left_pixel = [0; 4];
                                let mut src2 = prev_line;
                                for _x in 0..width {
                                    let r = buffer[src]
                                        .wrapping_add(average(left_pixel[0], buffer[src2]));
                                    let g = buffer[src + 1]
                                        .wrapping_add(average(left_pixel[1], buffer[src2 + 1]));
                                    let b = buffer[src + 2]
                                        .wrapping_add(average(left_pixel[2], buffer[src2 + 2]));
                                    compiler_fence(Ordering::SeqCst);
                                    buffer[dest] = r;
                                    buffer[dest + 1] = g;
                                    buffer[dest + 2] = b;
                                    left_pixel[0] = r;
                                    left_pixel[1] = g;
                                    left_pixel[2] = b;
                                    dest += 3;
                                    src += 3;
                                    src2 += 3;
                                }
                            } else {
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src];
                                    dest += 1;
                                    src += 1;
                                }
                            }
                        }
                        // Recon(x) = Filt(x) + PaethPredictor(Recon(a), Recon(b), Recon(c))
                        FilterType::Paeth => {
                            if let Some(prev_line) = prev_line {
                                let mut left_pixel = [0; 4];
                                let mut upper_left = [0; 4];
                                let mut src2 = prev_line;
                                for _x in 0..width {
                                    let (above_r, above_g, above_b) =
                                        (buffer[src2], buffer[src2 + 1], buffer[src2 + 2]);
                                    let r = buffer[src].wrapping_add(paeth(
                                        left_pixel[0],
                                        above_r,
                                        upper_left[0],
                                    ));
                                    let g = buffer[src + 1].wrapping_add(paeth(
                                        left_pixel[1],
                                        above_g,
                                        upper_left[1],
                                    ));
                                    let b = buffer[src + 2].wrapping_add(paeth(
                                        left_pixel[2],
                                        above_b,
                                        upper_left[2],
                                    ));
                                    compiler_fence(Ordering::SeqCst);
                                    buffer[dest] = r;
                                    buffer[dest + 1] = g;
                                    buffer[dest + 2] = b;
                                    left_pixel[0] = r;
                                    left_pixel[1] = g;
                                    left_pixel[2] = b;
                                    upper_left[0] = above_r;
                                    upper_left[1] = above_g;
                                    upper_left[2] = above_b;
                                    dest += 3;
                                    src += 3;
                                    src2 += 3;
                                }
                            } else {
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src];
                                    dest += 1;
                                    src += 1;
                                }
                            }
                        }
                    }
                    prev_line = Some(this_line);
                }
            }
            NumberOfChannnels::Four => {
                let mut dest = 0;
                let mut src = 0;
                let mut prev_line = Option::<usize>::None;
                for _y in 0..self.info.height {
                    let filter_type = buffer[src];
                    let filter_type =
                        FilterType::new(filter_type).ok_or(DecodeError::InvalidData)?;
                    src += 1;
                    let this_line = dest;
                    match filter_type {
                        // Recon(x) = Filt(x)
                        FilterType::None => {
                            for _x in 0..stride {
                                buffer[dest] = buffer[src];
                                dest += 1;
                                src += 1;
                            }
                        }
                        // Recon(x) = Filt(x) + Recon(a)
                        FilterType::Sub => {
                            let mut left_pixel = [0; 4];
                            for _x in 0..width {
                                let r = buffer[src].wrapping_add(left_pixel[0]);
                                let g = buffer[src + 1].wrapping_add(left_pixel[1]);
                                let b = buffer[src + 2].wrapping_add(left_pixel[2]);
                                let a = buffer[src + 3].wrapping_add(left_pixel[3]);
                                buffer[dest] = r;
                                buffer[dest + 1] = g;
                                buffer[dest + 2] = b;
                                buffer[dest + 3] = a;
                                left_pixel[0] = r;
                                left_pixel[1] = g;
                                left_pixel[2] = b;
                                left_pixel[3] = a;
                                dest += 4;
                                src += 4;
                            }
                        }
                        // Recon(x) = Filt(x) + Recon(b)
                        FilterType::Up => {
                            if let Some(prev_line) = prev_line {
                                let mut src2 = prev_line;
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src].wrapping_add(buffer[src2]);
                                    dest += 1;
                                    src += 1;
                                    src2 += 1;
                                }
                            } else {
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src];
                                    dest += 1;
                                    src += 1;
                                }
                            }
                        }
                        // Recon(x) = Filt(x) + floor((Recon(a) + Recon(b)) / 2)
                        FilterType::Average => {
                            if let Some(prev_line) = prev_line {
                                let mut left_pixel = [0; 4];
                                let mut src2 = prev_line;
                                for _x in 0..width {
                                    let r = buffer[src]
                                        .wrapping_add(average(left_pixel[0], buffer[src2]));
                                    let g = buffer[src + 1]
                                        .wrapping_add(average(left_pixel[1], buffer[src2 + 1]));
                                    let b = buffer[src + 2]
                                        .wrapping_add(average(left_pixel[2], buffer[src2 + 2]));
                                    let a = buffer[src + 3]
                                        .wrapping_add(average(left_pixel[3], buffer[src2 + 3]));
                                    compiler_fence(Ordering::SeqCst);
                                    buffer[dest] = r;
                                    buffer[dest + 1] = g;
                                    buffer[dest + 2] = b;
                                    buffer[dest + 3] = a;
                                    left_pixel[0] = r;
                                    left_pixel[1] = g;
                                    left_pixel[2] = b;
                                    left_pixel[3] = a;
                                    dest += 4;
                                    src += 4;
                                    src2 += 4;
                                }
                            } else {
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src];
                                    dest += 1;
                                    src += 1;
                                }
                            }
                        }
                        // Recon(x) = Filt(x) + PaethPredictor(Recon(a), Recon(b), Recon(c))
                        FilterType::Paeth => {
                            if let Some(prev_line) = prev_line {
                                let mut left_pixel = [0; 4];
                                let mut upper_left = [0; 4];
                                let mut src2 = prev_line;
                                for _x in 0..width {
                                    let (above_r, above_g, above_b, above_a) = (
                                        buffer[src2],
                                        buffer[src2 + 1],
                                        buffer[src2 + 2],
                                        buffer[src2 + 3],
                                    );
                                    let r = buffer[src].wrapping_add(paeth(
                                        left_pixel[0],
                                        above_r,
                                        upper_left[0],
                                    ));
                                    let g = buffer[src + 1].wrapping_add(paeth(
                                        left_pixel[1],
                                        above_g,
                                        upper_left[1],
                                    ));
                                    let b = buffer[src + 2].wrapping_add(paeth(
                                        left_pixel[2],
                                        above_b,
                                        upper_left[2],
                                    ));
                                    let a = buffer[src + 3].wrapping_add(paeth(
                                        left_pixel[3],
                                        above_a,
                                        upper_left[3],
                                    ));
                                    compiler_fence(Ordering::SeqCst);
                                    buffer[dest] = r;
                                    buffer[dest + 1] = g;
                                    buffer[dest + 2] = b;
                                    buffer[dest + 3] = a;
                                    left_pixel[0] = r;
                                    left_pixel[1] = g;
                                    left_pixel[2] = b;
                                    left_pixel[3] = a;
                                    upper_left[0] = above_r;
                                    upper_left[1] = above_g;
                                    upper_left[2] = above_b;
                                    upper_left[3] = above_a;
                                    dest += 4;
                                    src += 4;
                                    src2 += 4;
                                }
                            } else {
                                for _x in 0..stride {
                                    buffer[dest] = buffer[src];
                                    dest += 1;
                                    src += 1;
                                }
                            }
                        }
                    }
                    prev_line = Some(this_line);
                }
            }
        }

        unsafe {
            buffer.set_len(stride * self.info.height as usize);
        }
        // buffer.truncate(stride * self.info.height as usize);

        Ok(buffer)
    }
}

impl<DD: DeflateDecoder> core::fmt::Debug for CustomPngDecoder<'_, DD> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PngDecoder({:?})", self.info)
    }
}

struct PngChunksInner<'a> {
    slice: &'a [u8],
}

impl<'a> PngChunksInner<'a> {
    pub fn next_chunk(&mut self) -> Result<PngChunk<'a>, DecodeError> {
        let chunk = self.peek_chunk()?;
        let (_, next) = self
            .slice
            .split_at_checked(chunk.len() + 12)
            .ok_or(DecodeError::InvalidData)?;
        self.slice = next;
        Ok(chunk)
    }

    pub fn peek_chunk(&self) -> Result<PngChunk<'a>, DecodeError> {
        let (length, next) = self
            .slice
            .split_at_checked(4)
            .ok_or(DecodeError::InvalidData)?;
        let length = Be32(length.try_into().unwrap()).as_u32() as usize;
        let (chunk_type, next) = next.split_at_checked(4).ok_or(DecodeError::InvalidData)?;
        let chunk_type = FourCC(chunk_type.try_into().unwrap());
        if !chunk_type.is_valid() {
            return Err(DecodeError::InvalidData);
        }
        let (data, next) = next
            .split_at_checked(length)
            .ok_or(DecodeError::InvalidData)?;
        let (crc, _next) = next.split_at_checked(4).ok_or(DecodeError::InvalidData)?;
        let crc = Be32(crc[..4].try_into().unwrap()).as_u32();

        Ok(PngChunk {
            chunk_type,
            data,
            crc,
        })
    }
}

pub struct PngChunks<'a> {
    idat_count: usize,
    idat_size: usize,
    inner: PngChunksInner<'a>,
}

impl<'a> PngChunks<'a> {
    #[inline]
    pub const fn idat_count(&self) -> usize {
        self.idat_count
    }

    #[inline]
    pub const fn idat_size(&self) -> usize {
        self.idat_size
    }

    #[inline]
    pub fn peek(&self) -> Result<PngChunk<'a>, DecodeError> {
        self.inner.peek_chunk()
    }

    /// Look for IDAT chunks and merge buffers if necessary
    pub fn get_idat_chunks(&mut self, skip_plte: bool) -> Result<Cow<'a, [u8]>, DecodeError> {
        let idat_count = self.idat_count;
        let mut data: Option<Cow<'a, [u8]>> = if self.idat_count > 1 {
            Some(Cow::Owned(Vec::with_capacity(self.idat_size)))
        } else {
            None
        };
        if !skip_plte {
            loop {
                let chunk = self.peek()?;
                match chunk.chunk_type() {
                    FourCC::IDAT => break,
                    FourCC::PLTE => {}
                    _ => {
                        if chunk.chunk_type().is_critical() {
                            return Err(DecodeError::UnsupportedFormat);
                        }
                    }
                }
                self.next().ok_or(DecodeError::InvalidData)?;
            }
        }
        for chunk in self {
            if chunk.chunk_type() != FourCC::IDAT {
                if chunk.chunk_type().is_critical() {
                    return Err(DecodeError::UnsupportedFormat);
                }
                continue;
            }
            if idat_count > 1 {
                if let Some(Cow::Owned(data)) = data.as_mut() {
                    data.extend_from_slice(chunk.data());
                } else {
                    unreachable!()
                }
            } else {
                if data.is_some() {
                    unreachable!()
                }
                data = Some(Cow::Borrowed(chunk.data()));
            }
        }

        data.ok_or(DecodeError::InvalidData)
    }
}

impl<'a> Iterator for PngChunks<'a> {
    type Item = PngChunk<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let chunk = self.inner.next_chunk().unwrap();
        (!chunk.is_valid_iend()).then(|| chunk)
    }
}
