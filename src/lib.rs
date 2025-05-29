//! An implementation of PNG Encoder and Decoder
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
use compress::deflate;

pub mod color;

mod image_data;
pub use image_data::*;

mod decoder;
pub use decoder::*;
mod encoder;
pub use encoder::*;

#[cfg(test)]
mod tests;

pub const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\x0D\x0A\x1A\x0A";

pub const IHDR_SIZE: usize = 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    InvalidData,
    UnsupportedFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    InvalidInput,
    InvalidData,
}

pub struct PngChunk<'a> {
    chunk_type: FourCC,
    data: &'a [u8],
    crc: u32,
}

impl<'a> PngChunk<'a> {
    #[inline]
    pub fn new(chunk_type: FourCC, data: &'a [u8]) -> Self {
        #[allow(unused_mut)]
        let mut result = Self {
            chunk_type,
            data,
            crc: 0,
        };
        // result.crc = result.compute_crc();
        result
    }

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

    pub fn compute_crc(&self) -> u32 {
        let mut crc = crc::CRC::new();
        crc.update(&self.chunk_type.0);
        crc.update(self.data);
        crc.finalize()
    }

    #[inline]
    pub const fn is_iend(&self) -> bool {
        matches!(self.chunk_type, FourCC::IEND) && self.crc == 0xae426082
    }

    #[inline]
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn write_to(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&(self.len() as u32).to_be_bytes());
        buf.extend_from_slice(&self.chunk_type.0);
        buf.extend_from_slice(self.data);
        buf.extend_from_slice(&self.compute_crc().to_be_bytes());
    }
}

/// Represents a 32-bit unsigned integer in big-endian byte order.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Be32(pub [u8; 4]);

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

/// Represents a Four-Character Code (FourCC) used in PNG chunks.
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

    /// Convert to string.
    ///
    /// # Panics
    ///
    /// Panics if the FourCC contains invalid UTF-8 characters.
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

pub(crate) fn average(lhs: u8, rhs: u8) -> u8 {
    let avg = (lhs as u16 + rhs as u16) >> 1;
    avg as u8
}

/// Paeth predictor
///
/// Although the specification states that it is unsigned,
/// here it is calculated as a signed integer because the decoding result differs when calculated without a sign.
pub fn paeth(left: u8, above: u8, upper_left: u8) -> u8 {
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
