//! Q47.16 fixed-point scalar + 3-vector.
//!
//! Everything here is pure integer arithmetic (i64/i128), so results are
//! *bit-identical across architectures* — which is exactly what makes the
//! n-body checksum a good cross-arch determinism probe.

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Fixed(pub i64);

impl Fixed {
    pub const FRAC_BITS: u32 = 16;
    pub const SCALE: i64 = 1 << Self::FRAC_BITS;

    pub const fn from_int(n: i64) -> Self {
        Fixed(n << Self::FRAC_BITS)
    }

    pub const fn zero() -> Self {
        Fixed(0)
    }

    pub fn to_int(self) -> i64 {
        self.0 >> Self::FRAC_BITS
    }

    /// Newton's method square root in fixed point.
    pub fn sqrt(self) -> Self {
        if self.0 <= 0 {
            return Fixed::zero();
        }
        let mut x = self;
        for _ in 0..16 {
            let x2 = x * x;
            let diff = self - x2;
            let two_x = Fixed(x.0 << 1);
            if two_x.0 != 0 {
                x = Fixed(x.0 + diff.0 / (two_x.0 >> Self::FRAC_BITS));
            }
        }
        x
    }
}

impl core::ops::Add for Fixed {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Fixed(self.0.wrapping_add(rhs.0))
    }
}

impl core::ops::Sub for Fixed {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Fixed(self.0.wrapping_sub(rhs.0))
    }
}

impl core::ops::Mul for Fixed {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let result = (self.0 as i128 * rhs.0 as i128) >> Self::FRAC_BITS;
        Fixed(result as i64)
    }
}

impl core::ops::Div for Fixed {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        if rhs.0 == 0 {
            return Fixed(i64::MAX);
        }
        let result = ((self.0 as i128) << Self::FRAC_BITS) / rhs.0 as i128;
        Fixed(result as i64)
    }
}

impl core::ops::AddAssign for Fixed {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl core::ops::SubAssign for Fixed {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

#[derive(Clone, Copy, Default)]
pub struct Vec3 {
    pub x: Fixed,
    pub y: Fixed,
    pub z: Fixed,
}

impl Vec3 {
    pub const fn new(x: Fixed, y: Fixed, z: Fixed) -> Self {
        Vec3 { x, y, z }
    }

    pub fn magnitude_squared(self) -> Fixed {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    pub fn magnitude(self) -> Fixed {
        self.magnitude_squared().sqrt()
    }
}

impl core::ops::Add for Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Vec3::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl core::ops::Sub for Vec3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Vec3::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl core::ops::Mul<Fixed> for Vec3 {
    type Output = Self;
    fn mul(self, s: Fixed) -> Self {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

impl core::ops::AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}
