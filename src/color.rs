use core::mem::transmute;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RGBA8888(u32);

impl RGBA8888 {
    /// # SAFETY
    ///
    /// Byte order is dependent on the target architecture.
    #[inline]
    pub const unsafe fn from_inner(value: u32) -> Self {
        Self(value)
    }

    /// # SAFETY
    ///
    /// Byte order is dependent on the target architecture.
    #[inline]
    pub const unsafe fn into_inner(self) -> u32 {
        self.0
    }

    #[inline]
    pub const fn components(&self) -> RGBAComponents8888 {
        RGBAComponents8888::from_rgba(*self)
    }

    #[inline]
    pub const fn r(&self) -> u8 {
        self.components().r()
    }

    #[inline]
    pub const fn g(&self) -> u8 {
        self.components().g()
    }

    #[inline]
    pub const fn b(&self) -> u8 {
        self.components().b()
    }

    #[inline]
    pub const fn a(&self) -> u8 {
        self.components().a()
    }

    #[inline]
    pub const fn is_gray(&self) -> bool {
        let components = self.components();
        components.r() == components.g() && components.g() == components.b()
    }

    #[inline]
    pub const fn from_gray(gray: u8) -> Self {
        Self((gray as u32) * 0x00010101 | 0xFF000000)
    }

    #[inline]
    pub const fn from_gray_alpha(w: u8, a: u8) -> Self {
        Self((w as u32) * 0x00010101 | ((a as u32) << 24))
    }

    #[inline]
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        RGBAComponents8888::new(r, g, b, 0xFF).into_rgba()
    }

    #[inline]
    pub const fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        RGBAComponents8888::new(r, g, b, a).into_rgba()
    }

    #[inline]
    pub const fn to_rgb(&self) -> RGB888 {
        let components = self.components();
        RGB888 {
            r: components.r(),
            g: components.g(),
            b: components.b(),
        }
    }

    #[inline]
    pub const fn wrapping_add(&self, other: Self) -> Self {
        RGBAComponents8888::into_rgba(self.components().wrapping_add(other.components()))
    }

    #[inline]
    pub const fn wrapping_sub(&self, other: Self) -> Self {
        RGBAComponents8888::into_rgba(self.components().wrapping_sub(other.components()))
    }

    #[inline]
    pub const fn saturating_add(&self, other: Self) -> Self {
        RGBAComponents8888::into_rgba(self.components().saturating_add(other.components()))
    }

    #[inline]
    pub const fn saturating_sub(&self, other: Self) -> Self {
        RGBAComponents8888::into_rgba(self.components().saturating_sub(other.components()))
    }
}

impl PartialOrd for RGBA8888 {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.components().partial_cmp(&other.components())
    }
}

impl Ord for RGBA8888 {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.components().cmp(&other.components())
    }
}

#[cfg(target_endian = "little")]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RGBAComponents8888 {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl RGBAComponents8888 {
    #[inline]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
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
    pub const fn from_rgba(value: RGBA8888) -> Self {
        unsafe { transmute(value) }
    }

    #[inline]
    pub const fn into_rgba(self) -> RGBA8888 {
        unsafe { transmute(self) }
    }

    /// Converts color components to a byte array
    ///
    /// # SAFETY
    ///
    /// Mutual conversion is possible with `transmute`, but the order of values is undefined.
    #[inline]
    pub unsafe fn into_inner(self) -> [u8; 4] {
        unsafe { transmute(self) }
    }

    /// Converts color components from a byte array
    ///
    /// # SAFETY
    ///
    /// Mutual conversion is possible with `transmute`, but the order of values is undefined.
    #[inline]
    pub unsafe fn from_inner(bytes: [u8; 4]) -> Self {
        unsafe { transmute(bytes) }
    }

    #[inline]
    const fn _ordinal(&self) -> u32 {
        ((self.a as u32) << 24) | ((self.b as u32) << 16) | ((self.g as u32) << 8) | (self.r as u32)
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

impl PartialOrd for RGBAComponents8888 {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self._ordinal().partial_cmp(&other._ordinal())
    }
}

impl Ord for RGBAComponents8888 {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self._ordinal().cmp(&other._ordinal())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
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
        let components = RGBAComponents8888::from_rgba(rgba);
        Self {
            r: components.r(),
            g: components.g(),
            b: components.b(),
        }
    }

    #[inline]
    pub const fn into_rgba(self) -> RGBA8888 {
        RGBAComponents8888::new(self.r, self.g, self.b, 0xFF).into_rgba()
    }

    #[inline]
    pub const fn value(&self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    #[inline]
    pub const fn from_gray(gray: u8) -> Self {
        Self {
            r: gray,
            g: gray,
            b: gray,
        }
    }
}
