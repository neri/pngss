//! A subset implementation of the PNG decoder
//!
//! ## Features
//!
//! * Pure Rust Implementation
//! * Support for `no_std`
//! * It generally provides sufficient functionality for most applications, but some features are not supported.
//!
//! See also: <https://www.w3.org/TR/png/>

#![cfg_attr(not(test), no_std)]

extern crate alloc;
use alloc::borrow::Cow;
use alloc::vec::Vec;
use color::RGB888;
use compress::deflate::Deflate;

pub mod color;

mod image_data;
pub use image_data::*;

pub const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\x0D\x0A\x1A\x0A";

pub const IHDR_SIZE: usize = 13;

pub struct PngDecoder<'a> {
    slice: &'a [u8],
    info: ImageInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    InvalidData,
    UnsupportedFormat,
}

impl<'a> PngDecoder<'a> {
    /// Generates a PNG decoder from the specified slice.
    ///
    /// Returns an error if the signature is invalid, if the IHDR chunk does not exist, or if it has unsupported information.
    pub fn new(input: &'a [u8]) -> Result<PngDecoder<'a>, DecodeError> {
        let (signature, next) = input.split_at_checked(8).ok_or(DecodeError::InvalidData)?;
        if signature != PNG_SIGNATURE {
            return Err(DecodeError::InvalidData);
        }

        let (ihdr, next) = next
            .split_at_checked(12 + IHDR_SIZE)
            .ok_or(DecodeError::InvalidData)?;
        let mut ihdr = PngChunksInner { slice: ihdr };
        let ihdr = ihdr.next_chunk()?;
        if ihdr.chunk_type() != FourCC::IHDR {
            return Err(DecodeError::InvalidData);
        }
        if ihdr.len() != IHDR_SIZE {
            return Err(DecodeError::InvalidData);
        }

        let width = Be32(ihdr.data()[0..4].try_into().unwrap()).as_u32();
        let height = Be32(ihdr.data()[4..8].try_into().unwrap()).as_u32();
        if width == 0 || height == 0 {
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
        let image_type = match (color_type, bit_depth) {
            (0, BitDepth::Bpp8) => ImageType::Grayscale,
            (2, BitDepth::Bpp8) => ImageType::RGB,
            (3, BitDepth::Bpp1)
            | (3, BitDepth::Bpp2)
            | (3, BitDepth::Bpp4)
            | (3, BitDepth::Bpp8) => ImageType::Indexed,
            (4, BitDepth::Bpp8) => ImageType::GrayscaleAlpha,
            (6, BitDepth::Bpp8) => ImageType::RGBA,
            _ => return Err(DecodeError::UnsupportedFormat),
        };

        let compression_method = ihdr.data()[10];
        let filter_method = ihdr.data()[11];
        let interlace_method = ihdr.data()[12];
        if compression_method != 0 || filter_method != 0 || interlace_method != 0 {
            return Err(DecodeError::UnsupportedFormat);
        }

        let info = ImageInfo {
            width,
            height,
            bit_depth,
            image_type,
        };

        Ok(PngDecoder { slice: next, info })
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
            if chunk.is_iend() {
                break;
            }
        }

        Ok(PngChunks {
            idat_count,
            idat_size,
            inner: self.chunks_unchecked(),
        })
    }

    #[inline]
    pub fn info(&self) -> &ImageInfo {
        &self.info
    }

    #[inline]
    pub fn decoded_buffer_size(&self) -> usize {
        (1 + self.info.width as usize * self.info.image_type.n_channels() as usize)
            * self.info.height as usize
    }

    #[inline(always)]
    pub fn inflate(input: &[u8], size: usize) -> Result<Vec<u8>, DecodeError> {
        Deflate::inflate(input, size).map_err(|_| DecodeError::InvalidData)
    }

    /// Decodes PNG images and returns image data.
    pub fn decode(&self) -> Result<ImageData, DecodeError> {
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
        let inflated = Self::inflate(&data, self.decoded_buffer_size())?;

        // process filters
        let stride = if self.info.bit_depth > BitDepth::Bpp8 {
            self.info.width as usize * self.info.image_type.n_channels() as usize
        } else {
            (self.info.width as usize
                * self.info.image_type.n_channels() as usize
                * self.info.bit_depth as usize
                + 7)
                / 8
        };
        let mut source = inflated.as_slice();
        let mut reconstructed = Vec::with_capacity(stride * self.info.height as usize);
        let mut prev_line = Vec::with_capacity(stride);
        let mut line = Vec::with_capacity(stride);
        for _y in 0..self.info.height as usize {
            let Some((filter_type, next)) = source.split_at_checked(1) else {
                return Err(DecodeError::InvalidData);
            };
            let filter_type = FilterType::new(filter_type[0]).ok_or(DecodeError::InvalidData)?;
            let Some((line_src, next)) = next.split_at_checked(stride) else {
                return Err(DecodeError::InvalidData);
            };
            line.clear();
            match filter_type {
                FilterType::None => {
                    line.extend_from_slice(line_src);
                }
                FilterType::Sub => match self.info.image_type.n_channels() {
                    1 => {
                        let mut prev = 0;
                        for &byte in line_src.iter() {
                            let byte = byte.wrapping_add(prev);
                            line.push(byte);
                            prev = byte;
                        }
                    }
                    2 => {
                        let mut prev_y = 0;
                        let mut prev_a = 0;
                        for tuple in line_src.chunks_exact(2) {
                            let (y, a) = (tuple[0], tuple[1]);
                            let y = y.wrapping_add(prev_y);
                            let a = a.wrapping_add(prev_a);
                            line.push(y);
                            line.push(a);
                            prev_y = y;
                            prev_a = a;
                        }
                    }
                    3 => {
                        let mut prev_r = 0;
                        let mut prev_g = 0;
                        let mut prev_b = 0;
                        for tuple in line_src.chunks_exact(3) {
                            let (r, g, b) = (tuple[0], tuple[1], tuple[2]);
                            let r = r.wrapping_add(prev_r);
                            let g = g.wrapping_add(prev_g);
                            let b = b.wrapping_add(prev_b);
                            line.push(r);
                            line.push(g);
                            line.push(b);
                            prev_r = r;
                            prev_g = g;
                            prev_b = b;
                        }
                    }
                    4 => {
                        let mut prev_r = 0;
                        let mut prev_g = 0;
                        let mut prev_b = 0;
                        let mut prev_a = 0;
                        for tuple in line_src.chunks_exact(4) {
                            let (r, g, b, a) = (tuple[0], tuple[1], tuple[2], tuple[3]);
                            let r = r.wrapping_add(prev_r);
                            let g = g.wrapping_add(prev_g);
                            let b = b.wrapping_add(prev_b);
                            let a = a.wrapping_add(prev_a);
                            line.push(r);
                            line.push(g);
                            line.push(b);
                            line.push(a);
                            prev_r = r;
                            prev_g = g;
                            prev_b = b;
                            prev_a = a;
                        }
                    }
                    _ => unreachable!(),
                },
                FilterType::Up => {
                    if prev_line.is_empty() {
                        line.extend_from_slice(line_src);
                    } else {
                        for (&x, &above) in line_src.iter().zip(prev_line.iter()) {
                            line.push(x.wrapping_add(above));
                        }
                    }
                }
                FilterType::Average => match self.info.image_type.n_channels() {
                    1 => {
                        let mut prev = 0;
                        for (x, &above) in line_src.iter().zip(prev_line.iter()) {
                            let x = x.wrapping_add(average(above, prev));
                            line.push(x);
                            prev = x;
                        }
                    }
                    2 => {
                        let mut prev_y = 0;
                        let mut prev_a = 0;
                        for (x, above) in line_src.chunks_exact(2).zip(prev_line.chunks_exact(2)) {
                            let (y, a) = (x[0], x[1]);
                            let (a_y, a_a) = (above[0], above[1]);
                            let y = y.wrapping_add(average(a_y, prev_y));
                            let a = a.wrapping_add(average(a_a, prev_a));
                            line.push(y);
                            line.push(a);
                            prev_y = y;
                            prev_a = a;
                        }
                    }
                    3 => {
                        let mut prev_r = 0;
                        let mut prev_g = 0;
                        let mut prev_b = 0;
                        for (x, above) in line_src.chunks_exact(3).zip(prev_line.chunks_exact(3)) {
                            let (r, g, b) = (x[0], x[1], x[2]);
                            let (a_r, a_g, a_b) = (above[0], above[1], above[2]);
                            let r = r.wrapping_add(average(a_r, prev_r));
                            let g = g.wrapping_add(average(a_g, prev_g));
                            let b = b.wrapping_add(average(a_b, prev_b));
                            line.push(r);
                            line.push(g);
                            line.push(b);
                            prev_r = r;
                            prev_g = g;
                            prev_b = b;
                        }
                    }
                    4 => {
                        let mut prev_r = 0;
                        let mut prev_g = 0;
                        let mut prev_b = 0;
                        let mut prev_a = 0;
                        for (x, above) in line_src.chunks_exact(4).zip(prev_line.chunks_exact(4)) {
                            let (r, g, b, a) = (x[0], x[1], x[2], x[3]);
                            let (a_r, a_g, a_b, a_a) = (above[0], above[1], above[2], above[3]);
                            let r = r.wrapping_add(average(a_r, prev_r));
                            let g = g.wrapping_add(average(a_g, prev_g));
                            let b = b.wrapping_add(average(a_b, prev_b));
                            let a = a.wrapping_add(average(a_a, prev_a));
                            line.push(r);
                            line.push(g);
                            line.push(b);
                            line.push(a);
                            prev_r = r;
                            prev_g = g;
                            prev_b = b;
                            prev_a = a;
                        }
                    }
                    _ => unreachable!(),
                },
                FilterType::Paeth => match self.info.image_type.n_channels() {
                    1 => {
                        let mut left = 0;
                        let mut upper_left = 0;
                        for (x, &above) in line_src.iter().zip(prev_line.iter()) {
                            let x = x.wrapping_add(paeth(left, above, upper_left));
                            line.push(x);
                            left = x;
                            upper_left = above;
                        }
                    }
                    2 => {
                        let mut left_y = 0;
                        let mut left_a = 0;
                        let mut upper_left_y = 0;
                        let mut upper_left_a = 0;
                        for (x, above) in line_src.chunks_exact(2).zip(prev_line.chunks_exact(2)) {
                            let (y, a) = (x[0], x[1]);
                            let (a_y, a_a) = (above[0], above[1]);
                            let y = y.wrapping_add(paeth(left_y, a_y, upper_left_y));
                            let a = a.wrapping_add(paeth(left_a, a_a, upper_left_a));
                            line.push(y);
                            line.push(a);
                            left_y = y;
                            left_a = a;
                            upper_left_y = a_y;
                            upper_left_a = a_a;
                        }
                    }
                    3 => {
                        let mut left_r = 0;
                        let mut left_g = 0;
                        let mut left_b = 0;
                        let mut upper_left_r = 0;
                        let mut upper_left_g = 0;
                        let mut upper_left_b = 0;
                        for (x, above) in line_src.chunks_exact(3).zip(prev_line.chunks_exact(3)) {
                            let (r, g, b) = (x[0], x[1], x[2]);
                            let (a_r, a_g, a_b) = (above[0], above[1], above[2]);
                            let r = r.wrapping_add(paeth(left_r, a_r, upper_left_r));
                            let g = g.wrapping_add(paeth(left_g, a_g, upper_left_g));
                            let b = b.wrapping_add(paeth(left_b, a_b, upper_left_b));
                            line.push(r);
                            line.push(g);
                            line.push(b);
                            left_r = r;
                            left_g = g;
                            left_b = b;
                            upper_left_r = a_r;
                            upper_left_g = a_g;
                            upper_left_b = a_b;
                        }
                    }
                    4 => {
                        let mut left_r = 0;
                        let mut left_g = 0;
                        let mut left_b = 0;
                        let mut left_a = 0;
                        let mut upper_left_r = 0;
                        let mut upper_left_g = 0;
                        let mut upper_left_b = 0;
                        let mut upper_left_a = 0;
                        for (x, above) in line_src.chunks_exact(4).zip(prev_line.chunks_exact(4)) {
                            let (r, g, b, a) = (x[0], x[1], x[2], x[3]);
                            let (a_r, a_g, a_b, a_a) = (above[0], above[1], above[2], above[3]);
                            let r = r.wrapping_add(paeth(left_r, a_r, upper_left_r));
                            let g = g.wrapping_add(paeth(left_g, a_g, upper_left_g));
                            let b = b.wrapping_add(paeth(left_b, a_b, upper_left_b));
                            let a = a.wrapping_add(paeth(left_a, a_a, upper_left_a));
                            line.push(r);
                            line.push(g);
                            line.push(b);
                            line.push(a);
                            left_r = r;
                            left_g = g;
                            left_b = b;
                            left_a = a;
                            upper_left_r = a_r;
                            upper_left_g = a_g;
                            upper_left_b = a_b;
                            upper_left_a = a_a;
                        }
                    }
                    _ => unreachable!(),
                },
            }
            reconstructed.extend_from_slice(&line);
            core::mem::swap(&mut line, &mut prev_line);
            source = next;
        }

        // fix bit depth less than 8
        if self.info.bit_depth < BitDepth::Bpp8 {
            let mut fixed =
                Vec::with_capacity(self.info.width as usize * self.info.height as usize);
            match self.info.bit_depth {
                BitDepth::Bpp1 => {
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
                BitDepth::Bpp2 => {
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
                BitDepth::Bpp4 => {
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
                BitDepth::Bpp8 => {
                    unreachable!()
                }
            }
            reconstructed = fixed;
        }

        // pallete check
        if self.info.image_type == ImageType::Indexed {
            let Some(palette) = palette.as_ref() else {
                return Err(DecodeError::InvalidData);
            };
            let max_index = reconstructed.iter().copied().max().unwrap() as usize;
            if palette.len() > 256 || max_index >= palette.len() {
                return Err(DecodeError::InvalidData);
            }
        }

        // return the image data
        Ok(ImageData {
            info: self.info,
            palette: palette.unwrap_or_default(),
            data: reconstructed,
        })
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
        (!chunk.is_iend()).then(|| chunk)
    }
}

pub struct PngChunk<'a> {
    chunk_type: FourCC,
    data: &'a [u8],
    crc: u32,
}

impl<'a> PngChunk<'a> {
    #[inline]
    pub const fn len(&self) -> usize {
        self.data.len()
    }

    #[inline]
    pub const fn chunk_type(&self) -> FourCC {
        self.chunk_type
    }

    #[inline]
    pub const fn crc(&self) -> u32 {
        self.crc
    }

    #[inline]
    pub const fn is_iend(&self) -> bool {
        matches!(self.chunk_type, FourCC::IEND)
    }

    #[inline]
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Be32([u8; 4]);

impl Be32 {
    #[inline]
    pub const fn from_u32(value: u32) -> Self {
        Self(value.to_be_bytes())
    }

    #[inline]
    pub const fn as_u32(&self) -> u32 {
        u32::from_be_bytes(self.0)
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FourCC(pub [u8; 4]);

#[allow(non_upper_case_globals)]
impl FourCC {
    pub const IHDR: Self = Self(*b"IHDR");

    pub const PLTE: Self = Self(*b"PLTE");

    pub const IDAT: Self = Self(*b"IDAT");

    pub const IEND: Self = Self(*b"IEND");
}

impl FourCC {
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.0[0].is_ascii_alphabetic()
            && self.0[1].is_ascii_alphabetic()
            && self.0[2].is_ascii_alphabetic()
            && self.0[3].is_ascii_alphabetic()
            && !self.is_reserved()
    }

    #[inline]
    pub const fn is_ancillary(&self) -> bool {
        self.0[0] & 0x20 != 0
    }

    #[inline]
    pub const fn is_critical(&self) -> bool {
        !self.is_ancillary()
    }

    #[inline]
    pub const fn is_private(&self) -> bool {
        self.0[1] & 0x20 != 0
    }

    #[inline]
    pub const fn is_public(&self) -> bool {
        !self.is_private()
    }

    #[inline]
    pub const fn is_reserved(&self) -> bool {
        self.0[2] & 0x20 != 0
    }

    #[inline]
    pub const fn is_safe_to_copy(&self) -> bool {
        self.0[3] & 0x20 != 0
    }

    #[inline]
    pub const fn is_unsafe_to_copy(&self) -> bool {
        !self.is_safe_to_copy()
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.0).unwrap()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterType {
    /// ```plain
    /// Filt(x) = Orig(x)
    /// Recon(x) = Filt(x)
    /// ```
    None,
    /// ```plain
    /// Filt(x) = Orig(x) - Orig(a)
    /// Recon(x) = Filt(x) + Recon(a)
    /// ```
    Sub,
    /// ```plain
    /// Filt(x) = Orig(x) - Orig(b)
    /// Recon(x) = Filt(x) + Recon(b)
    /// ```
    Up,
    /// ```plain
    /// Filt(x) = Orig(x) - floor((Orig(a) + Orig(b)) / 2)
    /// Recon(x) = Filt(x) + floor((Recon(a) + Recon(b)) / 2)
    /// ```
    Average,
    /// ```plain
    /// Filt(x) = Orig(x) - PaethPredictor(Orig(a), Orig(b), Orig(c))
    /// Recon(x) = Filt(x) + PaethPredictor(Recon(a), Recon(b), Recon(c))
    /// ```
    Paeth,
}

impl FilterType {
    #[inline]
    pub fn new(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Sub),
            2 => Some(Self::Up),
            3 => Some(Self::Average),
            4 => Some(Self::Paeth),
            _ => None,
        }
    }
}

fn average(lhs: u8, rhs: u8) -> u8 {
    let avg = (lhs as u16 + rhs as u16) >> 1;
    avg as u8
}

/// Paeth predictor
///
/// Although the specification states that it is unsigned,
/// here it is calculated as a signed integer because the decoding result differs when calculated without a sign.
fn paeth(left: u8, above: u8, upper_left: u8) -> u8 {
    let a = left as i32;
    let b = above as i32;
    let c = upper_left as i32;
    let p = a.wrapping_add(b).wrapping_sub(c);
    let pa = p.abs_diff(a);
    let pb = p.abs_diff(b);
    let pc = p.abs_diff(c);
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

#[test]
fn it_works() {}
