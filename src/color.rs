//! Color types

#[repr(C, align(4))]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RGBA8888 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RGBA8888 {
    #[inline]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    #[inline]
    pub const fn from_gray(y: u8) -> Self {
        Self {
            r: y,
            g: y,
            b: y,
            a: 0xff,
        }
    }

    #[inline]
    pub const fn from_gray_alpha(y: u8, a: u8) -> Self {
        Self {
            r: y,
            g: y,
            b: y,
            a: a,
        }
    }

    #[inline]
    pub const fn r(&self) -> u8 {
        self.r
    }

    #[inline]
    pub const fn g(&self) -> u8 {
        self.g
    }

    #[inline]
    pub const fn b(&self) -> u8 {
        self.b
    }

    #[inline]
    pub const fn a(&self) -> u8 {
        self.a
    }

    #[inline]
    pub const fn into_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    #[inline]
    pub const fn from_array(arr: [u8; 4]) -> Self {
        Self {
            r: arr[0],
            g: arr[1],
            b: arr[2],
            a: arr[3],
        }
    }

    #[inline]
    const fn _ordinal(&self) -> u32 {
        u32::from_le_bytes([self.r, self.g, self.b, self.a])
    }

    #[inline]
    pub const fn wrapping_add(&self, other: Self) -> Self {
        Self {
            r: self.r.wrapping_add(other.r),
            g: self.g.wrapping_add(other.g),
            b: self.b.wrapping_add(other.b),
            a: self.a.wrapping_add(other.a),
        }
    }

    #[inline]
    pub const fn wrapping_sub(&self, other: Self) -> Self {
        Self {
            r: self.r.wrapping_sub(other.r),
            g: self.g.wrapping_sub(other.g),
            b: self.b.wrapping_sub(other.b),
            a: self.a.wrapping_sub(other.a),
        }
    }

    #[inline]
    pub const fn saturating_add(&self, other: Self) -> Self {
        Self {
            r: self.r.saturating_add(other.r),
            g: self.g.saturating_add(other.g),
            b: self.b.saturating_add(other.b),
            a: self.a.saturating_add(other.a),
        }
    }

    #[inline]
    pub const fn saturating_sub(&self, other: Self) -> Self {
        Self {
            r: self.r.saturating_sub(other.r),
            g: self.g.saturating_sub(other.g),
            b: self.b.saturating_sub(other.b),
            a: self.a.saturating_sub(other.a),
        }
    }

    #[inline]
    pub const fn saturating_mul(&self, other: u8) -> Self {
        Self {
            r: self.r.saturating_mul(other),
            g: self.g.saturating_mul(other),
            b: self.b.saturating_mul(other),
            a: self.a.saturating_mul(other),
        }
    }
}

impl PartialOrd for RGBA8888 {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self._ordinal().partial_cmp(&other._ordinal())
    }
}

impl Ord for RGBA8888 {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self._ordinal().cmp(&other._ordinal())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RGB888 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RGB888 {
    #[inline]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    #[inline]
    pub const fn from_rgba(rgba: RGBA8888) -> Self {
        Self {
            r: rgba.r(),
            g: rgba.g(),
            b: rgba.b(),
        }
    }

    #[inline]
    pub const fn into_rgba(self) -> RGBA8888 {
        RGBA8888::new(self.r, self.g, self.b, 0xFF)
    }

    #[inline]
    pub const fn ordinal(&self) -> u32 {
        u32::from_le_bytes([self.r, self.g, self.b, 0])
    }

    #[inline]
    pub const fn from_gray(y: u8) -> Self {
        Self { r: y, g: y, b: y }
    }

    #[inline]
    pub const fn is_gray(&self) -> bool {
        self.r == self.g && self.g == self.b
    }
}

impl PartialOrd for RGB888 {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.ordinal().partial_cmp(&other.ordinal())
    }
}

impl Ord for RGB888 {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.ordinal().cmp(&other.ordinal())
    }
}

impl From<RGBA8888> for RGB888 {
    #[inline]
    fn from(rgba: RGBA8888) -> Self {
        Self::from_rgba(rgba)
    }
}

impl From<RGB888> for RGBA8888 {
    #[inline]
    fn from(rgb: RGB888) -> Self {
        Self::new(rgb.r, rgb.g, rgb.b, 0xff)
    }
}
