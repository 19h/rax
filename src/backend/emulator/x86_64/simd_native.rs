//! Native SIMD passthrough with automatic fallback.
//!
//! This module provides host-native execution of guest SIMD instructions
//! when the host CPU supports them, with automatic fallback to scalar
//! emulation for unsupported instruction sets.
//!
//! Hierarchy (each level falls back to the next):
//! - AVX-512 -> AVX2 (two 256-bit ops) -> SSE4.1 (four 128-bit ops) -> Scalar
//! - AVX2 -> SSE4.1 -> Scalar
//! - SSE4.1 -> Scalar

use std::sync::OnceLock;

/// Cached host SIMD capabilities (detected once at startup)
#[derive(Clone, Copy, Debug)]
pub struct SimdCapabilities {
    pub sse2: bool,
    pub sse3: bool,
    pub ssse3: bool,
    pub sse4_1: bool,
    pub sse4_2: bool,
    pub avx: bool,
    pub avx2: bool,
    pub fma: bool,
    pub avx512f: bool,
    pub avx512bw: bool,
    pub avx512vl: bool,
}

/// Global cached capabilities
static HOST_CAPS: OnceLock<SimdCapabilities> = OnceLock::new();

impl SimdCapabilities {
    /// Detect host CPU capabilities (cached after first call)
    #[inline]
    pub fn get() -> &'static SimdCapabilities {
        HOST_CAPS.get_or_init(Self::detect)
    }

    fn detect() -> SimdCapabilities {
        #[cfg(target_arch = "x86_64")]
        {
            SimdCapabilities {
                sse2: std::arch::is_x86_feature_detected!("sse2"),
                sse3: std::arch::is_x86_feature_detected!("sse3"),
                ssse3: std::arch::is_x86_feature_detected!("ssse3"),
                sse4_1: std::arch::is_x86_feature_detected!("sse4.1"),
                sse4_2: std::arch::is_x86_feature_detected!("sse4.2"),
                avx: std::arch::is_x86_feature_detected!("avx"),
                avx2: std::arch::is_x86_feature_detected!("avx2"),
                fma: std::arch::is_x86_feature_detected!("fma"),
                avx512f: std::arch::is_x86_feature_detected!("avx512f"),
                avx512bw: std::arch::is_x86_feature_detected!("avx512bw"),
                avx512vl: std::arch::is_x86_feature_detected!("avx512vl"),
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            SimdCapabilities {
                sse2: false,
                sse3: false,
                ssse3: false,
                sse4_1: false,
                sse4_2: false,
                avx: false,
                avx2: false,
                fma: false,
                avx512f: false,
                avx512bw: false,
                avx512vl: false,
            }
        }
    }
}

// ============================================================================
// Type aliases for SIMD registers
// ============================================================================

/// XMM register as 128-bit array for SIMD operations
pub type Xmm = [u64; 2];

/// YMM register (256-bit) as [lo_xmm, hi_xmm]
pub type Ymm = [Xmm; 2];

/// ZMM register (512-bit) as [xmm0, xmm1, xmm2, xmm3]
pub type Zmm = [Xmm; 4];

// ============================================================================
// Helper to get capabilities without repeated calls
// ============================================================================

#[cfg(target_arch = "x86_64")]
macro_rules! with_caps {
    ($body:expr) => {{
        let caps = SimdCapabilities::get();
        $body
    }};
}

// ============================================================================
// FLOATING POINT ARITHMETIC (ADDPS, SUBPS, MULPS, DIVPS, etc.)
// ============================================================================

// --- ADDPS ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse")]
#[inline]
pub unsafe fn addps_native_sse(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_ps(dst.as_ptr() as *const f32);
    let b = _mm_loadu_ps(src.as_ptr() as *const f32);
    let c = _mm_add_ps(a, b);
    _mm_storeu_ps(dst.as_mut_ptr() as *mut f32, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
#[inline]
pub unsafe fn addps_native_avx(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_ps(dst.as_ptr() as *const f32);
    let b = _mm256_loadu_ps(src.as_ptr() as *const f32);
    let c = _mm256_add_ps(a, b);
    _mm256_storeu_ps(dst.as_mut_ptr() as *mut f32, c);
}

#[inline]
pub fn addps_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [f32; 4]) };
    let s = unsafe { &*(src as *const Xmm as *const [f32; 4]) };
    for i in 0..4 { d[i] += s[i]; }
}

#[inline]
pub fn addps_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { addps_native_sse(dst, src) };
            return;
        }
    }
    addps_scalar(dst, src);
}

#[inline]
pub fn addps_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx {
            unsafe { addps_native_avx(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                addps_native_sse(&mut dst[0], &src[0]);
                addps_native_sse(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    addps_scalar(&mut dst[0], &src[0]);
    addps_scalar(&mut dst[1], &src[1]);
}

// --- ADDPD ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn addpd_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_pd(dst.as_ptr() as *const f64);
    let b = _mm_loadu_pd(src.as_ptr() as *const f64);
    let c = _mm_add_pd(a, b);
    _mm_storeu_pd(dst.as_mut_ptr() as *mut f64, c);
}

#[inline]
pub fn addpd_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [f64; 2]) };
    let s = unsafe { &*(src as *const Xmm as *const [f64; 2]) };
    d[0] += s[0];
    d[1] += s[1];
}

#[inline]
pub fn addpd_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { addpd_native_sse2(dst, src) };
            return;
        }
    }
    addpd_scalar(dst, src);
}

// --- SUBPS ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse")]
#[inline]
pub unsafe fn subps_native_sse(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_ps(dst.as_ptr() as *const f32);
    let b = _mm_loadu_ps(src.as_ptr() as *const f32);
    let c = _mm_sub_ps(a, b);
    _mm_storeu_ps(dst.as_mut_ptr() as *mut f32, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
#[inline]
pub unsafe fn subps_native_avx(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_ps(dst.as_ptr() as *const f32);
    let b = _mm256_loadu_ps(src.as_ptr() as *const f32);
    let c = _mm256_sub_ps(a, b);
    _mm256_storeu_ps(dst.as_mut_ptr() as *mut f32, c);
}

#[inline]
pub fn subps_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [f32; 4]) };
    let s = unsafe { &*(src as *const Xmm as *const [f32; 4]) };
    for i in 0..4 { d[i] -= s[i]; }
}

#[inline]
pub fn subps_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { subps_native_sse(dst, src) };
            return;
        }
    }
    subps_scalar(dst, src);
}

#[inline]
pub fn subps_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx {
            unsafe { subps_native_avx(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                subps_native_sse(&mut dst[0], &src[0]);
                subps_native_sse(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    subps_scalar(&mut dst[0], &src[0]);
    subps_scalar(&mut dst[1], &src[1]);
}

// --- SUBPD ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn subpd_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_pd(dst.as_ptr() as *const f64);
    let b = _mm_loadu_pd(src.as_ptr() as *const f64);
    let c = _mm_sub_pd(a, b);
    _mm_storeu_pd(dst.as_mut_ptr() as *mut f64, c);
}

#[inline]
pub fn subpd_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [f64; 2]) };
    let s = unsafe { &*(src as *const Xmm as *const [f64; 2]) };
    d[0] -= s[0];
    d[1] -= s[1];
}

#[inline]
pub fn subpd_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { subpd_native_sse2(dst, src) };
            return;
        }
    }
    subpd_scalar(dst, src);
}

// --- MULPS ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse")]
#[inline]
pub unsafe fn mulps_native_sse(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_ps(dst.as_ptr() as *const f32);
    let b = _mm_loadu_ps(src.as_ptr() as *const f32);
    let c = _mm_mul_ps(a, b);
    _mm_storeu_ps(dst.as_mut_ptr() as *mut f32, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
#[inline]
pub unsafe fn mulps_native_avx(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_ps(dst.as_ptr() as *const f32);
    let b = _mm256_loadu_ps(src.as_ptr() as *const f32);
    let c = _mm256_mul_ps(a, b);
    _mm256_storeu_ps(dst.as_mut_ptr() as *mut f32, c);
}

#[inline]
pub fn mulps_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [f32; 4]) };
    let s = unsafe { &*(src as *const Xmm as *const [f32; 4]) };
    for i in 0..4 { d[i] *= s[i]; }
}

#[inline]
pub fn mulps_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { mulps_native_sse(dst, src) };
            return;
        }
    }
    mulps_scalar(dst, src);
}

#[inline]
pub fn mulps_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx {
            unsafe { mulps_native_avx(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                mulps_native_sse(&mut dst[0], &src[0]);
                mulps_native_sse(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    mulps_scalar(&mut dst[0], &src[0]);
    mulps_scalar(&mut dst[1], &src[1]);
}

// --- MULPD ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn mulpd_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_pd(dst.as_ptr() as *const f64);
    let b = _mm_loadu_pd(src.as_ptr() as *const f64);
    let c = _mm_mul_pd(a, b);
    _mm_storeu_pd(dst.as_mut_ptr() as *mut f64, c);
}

#[inline]
pub fn mulpd_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [f64; 2]) };
    let s = unsafe { &*(src as *const Xmm as *const [f64; 2]) };
    d[0] *= s[0];
    d[1] *= s[1];
}

#[inline]
pub fn mulpd_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { mulpd_native_sse2(dst, src) };
            return;
        }
    }
    mulpd_scalar(dst, src);
}

// --- DIVPS ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse")]
#[inline]
pub unsafe fn divps_native_sse(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_ps(dst.as_ptr() as *const f32);
    let b = _mm_loadu_ps(src.as_ptr() as *const f32);
    let c = _mm_div_ps(a, b);
    _mm_storeu_ps(dst.as_mut_ptr() as *mut f32, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
#[inline]
pub unsafe fn divps_native_avx(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_ps(dst.as_ptr() as *const f32);
    let b = _mm256_loadu_ps(src.as_ptr() as *const f32);
    let c = _mm256_div_ps(a, b);
    _mm256_storeu_ps(dst.as_mut_ptr() as *mut f32, c);
}

#[inline]
pub fn divps_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [f32; 4]) };
    let s = unsafe { &*(src as *const Xmm as *const [f32; 4]) };
    for i in 0..4 { d[i] /= s[i]; }
}

#[inline]
pub fn divps_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { divps_native_sse(dst, src) };
            return;
        }
    }
    divps_scalar(dst, src);
}

#[inline]
pub fn divps_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx {
            unsafe { divps_native_avx(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                divps_native_sse(&mut dst[0], &src[0]);
                divps_native_sse(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    divps_scalar(&mut dst[0], &src[0]);
    divps_scalar(&mut dst[1], &src[1]);
}

// --- DIVPD ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn divpd_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_pd(dst.as_ptr() as *const f64);
    let b = _mm_loadu_pd(src.as_ptr() as *const f64);
    let c = _mm_div_pd(a, b);
    _mm_storeu_pd(dst.as_mut_ptr() as *mut f64, c);
}

#[inline]
pub fn divpd_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [f64; 2]) };
    let s = unsafe { &*(src as *const Xmm as *const [f64; 2]) };
    d[0] /= s[0];
    d[1] /= s[1];
}

#[inline]
pub fn divpd_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { divpd_native_sse2(dst, src) };
            return;
        }
    }
    divpd_scalar(dst, src);
}

// ============================================================================
// INTEGER ARITHMETIC (PADDB, PADDW, PADDD, PADDQ, PSUBB, etc.)
// ============================================================================

// --- PADDB ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn paddb_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_add_epi8(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn paddb_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_add_epi8(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn paddb_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u8; 16]) };
    let s = unsafe { &*(src as *const Xmm as *const [u8; 16]) };
    for i in 0..16 { d[i] = d[i].wrapping_add(s[i]); }
}

#[inline]
pub fn paddb_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { paddb_native_sse2(dst, src) };
            return;
        }
    }
    paddb_scalar(dst, src);
}

#[inline]
pub fn paddb_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { paddb_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                paddb_native_sse2(&mut dst[0], &src[0]);
                paddb_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    paddb_scalar(&mut dst[0], &src[0]);
    paddb_scalar(&mut dst[1], &src[1]);
}

// --- PADDW ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn paddw_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_add_epi16(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn paddw_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_add_epi16(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn paddw_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u16; 8]) };
    let s = unsafe { &*(src as *const Xmm as *const [u16; 8]) };
    for i in 0..8 { d[i] = d[i].wrapping_add(s[i]); }
}

#[inline]
pub fn paddw_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { paddw_native_sse2(dst, src) };
            return;
        }
    }
    paddw_scalar(dst, src);
}

#[inline]
pub fn paddw_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { paddw_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                paddw_native_sse2(&mut dst[0], &src[0]);
                paddw_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    paddw_scalar(&mut dst[0], &src[0]);
    paddw_scalar(&mut dst[1], &src[1]);
}

// --- PADDD ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn paddd_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_add_epi32(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn paddd_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_add_epi32(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn paddd_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u32; 4]) };
    let s = unsafe { &*(src as *const Xmm as *const [u32; 4]) };
    for i in 0..4 { d[i] = d[i].wrapping_add(s[i]); }
}

#[inline]
pub fn paddd_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { paddd_native_sse2(dst, src) };
            return;
        }
    }
    paddd_scalar(dst, src);
}

#[inline]
pub fn paddd_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { paddd_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                paddd_native_sse2(&mut dst[0], &src[0]);
                paddd_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    paddd_scalar(&mut dst[0], &src[0]);
    paddd_scalar(&mut dst[1], &src[1]);
}

// --- PADDQ ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn paddq_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_add_epi64(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn paddq_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_add_epi64(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn paddq_scalar(dst: &mut Xmm, src: &Xmm) {
    dst[0] = dst[0].wrapping_add(src[0]);
    dst[1] = dst[1].wrapping_add(src[1]);
}

#[inline]
pub fn paddq_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { paddq_native_sse2(dst, src) };
            return;
        }
    }
    paddq_scalar(dst, src);
}

#[inline]
pub fn paddq_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { paddq_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                paddq_native_sse2(&mut dst[0], &src[0]);
                paddq_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    paddq_scalar(&mut dst[0], &src[0]);
    paddq_scalar(&mut dst[1], &src[1]);
}

// --- PSUBB ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn psubb_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_sub_epi8(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn psubb_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_sub_epi8(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn psubb_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u8; 16]) };
    let s = unsafe { &*(src as *const Xmm as *const [u8; 16]) };
    for i in 0..16 { d[i] = d[i].wrapping_sub(s[i]); }
}

#[inline]
pub fn psubb_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { psubb_native_sse2(dst, src) };
            return;
        }
    }
    psubb_scalar(dst, src);
}

#[inline]
pub fn psubb_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { psubb_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                psubb_native_sse2(&mut dst[0], &src[0]);
                psubb_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    psubb_scalar(&mut dst[0], &src[0]);
    psubb_scalar(&mut dst[1], &src[1]);
}

// --- PSUBW ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn psubw_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_sub_epi16(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn psubw_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_sub_epi16(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn psubw_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u16; 8]) };
    let s = unsafe { &*(src as *const Xmm as *const [u16; 8]) };
    for i in 0..8 { d[i] = d[i].wrapping_sub(s[i]); }
}

#[inline]
pub fn psubw_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { psubw_native_sse2(dst, src) };
            return;
        }
    }
    psubw_scalar(dst, src);
}

#[inline]
pub fn psubw_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { psubw_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                psubw_native_sse2(&mut dst[0], &src[0]);
                psubw_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    psubw_scalar(&mut dst[0], &src[0]);
    psubw_scalar(&mut dst[1], &src[1]);
}

// --- PSUBD ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn psubd_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_sub_epi32(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn psubd_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_sub_epi32(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn psubd_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u32; 4]) };
    let s = unsafe { &*(src as *const Xmm as *const [u32; 4]) };
    for i in 0..4 { d[i] = d[i].wrapping_sub(s[i]); }
}

#[inline]
pub fn psubd_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { psubd_native_sse2(dst, src) };
            return;
        }
    }
    psubd_scalar(dst, src);
}

#[inline]
pub fn psubd_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { psubd_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                psubd_native_sse2(&mut dst[0], &src[0]);
                psubd_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    psubd_scalar(&mut dst[0], &src[0]);
    psubd_scalar(&mut dst[1], &src[1]);
}

// --- PSUBQ ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn psubq_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_sub_epi64(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn psubq_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_sub_epi64(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn psubq_scalar(dst: &mut Xmm, src: &Xmm) {
    dst[0] = dst[0].wrapping_sub(src[0]);
    dst[1] = dst[1].wrapping_sub(src[1]);
}

#[inline]
pub fn psubq_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { psubq_native_sse2(dst, src) };
            return;
        }
    }
    psubq_scalar(dst, src);
}

#[inline]
pub fn psubq_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { psubq_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                psubq_native_sse2(&mut dst[0], &src[0]);
                psubq_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    psubq_scalar(&mut dst[0], &src[0]);
    psubq_scalar(&mut dst[1], &src[1]);
}

// ============================================================================
// LOGICAL OPERATIONS (PAND, POR, PXOR, PANDN)
// ============================================================================

// --- PAND ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn pand_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_and_si128(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn pand_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_and_si256(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn pand_scalar(dst: &mut Xmm, src: &Xmm) {
    dst[0] &= src[0];
    dst[1] &= src[1];
}

#[inline]
pub fn pand_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { pand_native_sse2(dst, src) };
            return;
        }
    }
    pand_scalar(dst, src);
}

#[inline]
pub fn pand_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { pand_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                pand_native_sse2(&mut dst[0], &src[0]);
                pand_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    pand_scalar(&mut dst[0], &src[0]);
    pand_scalar(&mut dst[1], &src[1]);
}

// --- POR ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn por_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_or_si128(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn por_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_or_si256(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn por_scalar(dst: &mut Xmm, src: &Xmm) {
    dst[0] |= src[0];
    dst[1] |= src[1];
}

#[inline]
pub fn por_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { por_native_sse2(dst, src) };
            return;
        }
    }
    por_scalar(dst, src);
}

#[inline]
pub fn por_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { por_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                por_native_sse2(&mut dst[0], &src[0]);
                por_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    por_scalar(&mut dst[0], &src[0]);
    por_scalar(&mut dst[1], &src[1]);
}

// --- PXOR ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn pxor_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_xor_si128(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn pxor_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_xor_si256(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn pxor_scalar(dst: &mut Xmm, src: &Xmm) {
    dst[0] ^= src[0];
    dst[1] ^= src[1];
}

#[inline]
pub fn pxor_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { pxor_native_sse2(dst, src) };
            return;
        }
    }
    pxor_scalar(dst, src);
}

#[inline]
pub fn pxor_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { pxor_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                pxor_native_sse2(&mut dst[0], &src[0]);
                pxor_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    pxor_scalar(&mut dst[0], &src[0]);
    pxor_scalar(&mut dst[1], &src[1]);
}

// --- PANDN ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn pandn_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_andnot_si128(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn pandn_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_andnot_si256(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn pandn_scalar(dst: &mut Xmm, src: &Xmm) {
    dst[0] = !dst[0] & src[0];
    dst[1] = !dst[1] & src[1];
}

#[inline]
pub fn pandn_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { pandn_native_sse2(dst, src) };
            return;
        }
    }
    pandn_scalar(dst, src);
}

#[inline]
pub fn pandn_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { pandn_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                pandn_native_sse2(&mut dst[0], &src[0]);
                pandn_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    pandn_scalar(&mut dst[0], &src[0]);
    pandn_scalar(&mut dst[1], &src[1]);
}

// ============================================================================
// COMPARE OPERATIONS (PCMPEQB, PCMPEQW, PCMPEQD, PCMPGTB, etc.)
// ============================================================================

// --- PCMPEQB ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn pcmpeqb_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_cmpeq_epi8(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn pcmpeqb_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_cmpeq_epi8(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn pcmpeqb_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u8; 16]) };
    let s = unsafe { &*(src as *const Xmm as *const [u8; 16]) };
    for i in 0..16 {
        d[i] = if d[i] == s[i] { 0xFF } else { 0 };
    }
}

#[inline]
pub fn pcmpeqb_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { pcmpeqb_native_sse2(dst, src) };
            return;
        }
    }
    pcmpeqb_scalar(dst, src);
}

#[inline]
pub fn pcmpeqb_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { pcmpeqb_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                pcmpeqb_native_sse2(&mut dst[0], &src[0]);
                pcmpeqb_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    pcmpeqb_scalar(&mut dst[0], &src[0]);
    pcmpeqb_scalar(&mut dst[1], &src[1]);
}

// --- PCMPEQD ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn pcmpeqd_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_cmpeq_epi32(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn pcmpeqd_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_cmpeq_epi32(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn pcmpeqd_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u32; 4]) };
    let s = unsafe { &*(src as *const Xmm as *const [u32; 4]) };
    for i in 0..4 {
        d[i] = if d[i] == s[i] { 0xFFFFFFFF } else { 0 };
    }
}

#[inline]
pub fn pcmpeqd_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { pcmpeqd_native_sse2(dst, src) };
            return;
        }
    }
    pcmpeqd_scalar(dst, src);
}

#[inline]
pub fn pcmpeqd_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { pcmpeqd_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                pcmpeqd_native_sse2(&mut dst[0], &src[0]);
                pcmpeqd_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    pcmpeqd_scalar(&mut dst[0], &src[0]);
    pcmpeqd_scalar(&mut dst[1], &src[1]);
}

// ============================================================================
// SHUFFLE/PERMUTE OPERATIONS (PSHUFB, PSHUFD, etc.)
// ============================================================================

// --- PSHUFB ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "ssse3")]
#[inline]
pub unsafe fn pshufb_native_ssse3(dst: &mut Xmm, mask: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(mask.as_ptr() as *const __m128i);
    let c = _mm_shuffle_epi8(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn pshufb_native_avx2(dst: &mut Ymm, mask: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(mask.as_ptr() as *const __m256i);
    let c = _mm256_shuffle_epi8(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn pshufb_scalar(dst: &mut Xmm, mask: &Xmm) {
    let src_bytes = unsafe { *(dst as *const Xmm as *const [u8; 16]) };
    let mask_bytes = unsafe { &*(mask as *const Xmm as *const [u8; 16]) };
    let dst_bytes = unsafe { &mut *(dst as *mut Xmm as *mut [u8; 16]) };

    for i in 0..16 {
        let m = mask_bytes[i];
        dst_bytes[i] = if m & 0x80 != 0 {
            0
        } else {
            src_bytes[(m & 0x0F) as usize]
        };
    }
}

#[inline]
pub fn pshufb_xmm(dst: &mut Xmm, mask: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().ssse3 {
            unsafe { pshufb_native_ssse3(dst, mask) };
            return;
        }
    }
    pshufb_scalar(dst, mask);
}

#[inline]
pub fn pshufb_ymm(dst: &mut Ymm, mask: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { pshufb_native_avx2(dst, mask) };
            return;
        }
        if caps.ssse3 {
            // AVX2 VPSHUFB shuffles within each 128-bit lane independently
            unsafe {
                pshufb_native_ssse3(&mut dst[0], &mask[0]);
                pshufb_native_ssse3(&mut dst[1], &mask[1]);
            }
            return;
        }
    }
    pshufb_scalar(&mut dst[0], &mask[0]);
    pshufb_scalar(&mut dst[1], &mask[1]);
}

// ============================================================================
// MULTIPLY OPERATIONS (PMULLW, PMULLD, PMADDWD, etc.)
// ============================================================================

// --- PMULLW ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn pmullw_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_mullo_epi16(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn pmullw_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_mullo_epi16(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn pmullw_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u16; 8]) };
    let s = unsafe { &*(src as *const Xmm as *const [u16; 8]) };
    for i in 0..8 {
        d[i] = d[i].wrapping_mul(s[i]);
    }
}

#[inline]
pub fn pmullw_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { pmullw_native_sse2(dst, src) };
            return;
        }
    }
    pmullw_scalar(dst, src);
}

#[inline]
pub fn pmullw_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { pmullw_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                pmullw_native_sse2(&mut dst[0], &src[0]);
                pmullw_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    pmullw_scalar(&mut dst[0], &src[0]);
    pmullw_scalar(&mut dst[1], &src[1]);
}

// --- PMULLD ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse4.1")]
#[inline]
pub unsafe fn pmulld_native_sse41(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_mullo_epi32(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn pmulld_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_mullo_epi32(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn pmulld_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u32; 4]) };
    let s = unsafe { &*(src as *const Xmm as *const [u32; 4]) };
    for i in 0..4 {
        d[i] = d[i].wrapping_mul(s[i]);
    }
}

#[inline]
pub fn pmulld_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse4_1 {
            unsafe { pmulld_native_sse41(dst, src) };
            return;
        }
    }
    pmulld_scalar(dst, src);
}

#[inline]
pub fn pmulld_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { pmulld_native_avx2(dst, src) };
            return;
        }
        if caps.sse4_1 {
            unsafe {
                pmulld_native_sse41(&mut dst[0], &src[0]);
                pmulld_native_sse41(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    pmulld_scalar(&mut dst[0], &src[0]);
    pmulld_scalar(&mut dst[1], &src[1]);
}

// --- PMADDWD ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn pmaddwd_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_madd_epi16(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn pmaddwd_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_madd_epi16(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn pmaddwd_scalar(dst: &mut Xmm, src: &Xmm) {
    let a = unsafe { &*(dst as *const Xmm as *const [i16; 8]) };
    let b = unsafe { &*(src as *const Xmm as *const [i16; 8]) };
    let result = unsafe { &mut *(dst as *mut Xmm as *mut [i32; 4]) };

    for i in 0..4 {
        result[i] = (a[i*2] as i32) * (b[i*2] as i32) + (a[i*2+1] as i32) * (b[i*2+1] as i32);
    }
}

#[inline]
pub fn pmaddwd_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { pmaddwd_native_sse2(dst, src) };
            return;
        }
    }
    pmaddwd_scalar(dst, src);
}

#[inline]
pub fn pmaddwd_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { pmaddwd_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                pmaddwd_native_sse2(&mut dst[0], &src[0]);
                pmaddwd_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    pmaddwd_scalar(&mut dst[0], &src[0]);
    pmaddwd_scalar(&mut dst[1], &src[1]);
}

// ============================================================================
// MIN/MAX OPERATIONS (PMINUB, PMAXUB, PMINSW, PMAXSW, etc.)
// ============================================================================

// --- PMINUB ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn pminub_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_min_epu8(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn pminub_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_min_epu8(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn pminub_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u8; 16]) };
    let s = unsafe { &*(src as *const Xmm as *const [u8; 16]) };
    for i in 0..16 { d[i] = d[i].min(s[i]); }
}

#[inline]
pub fn pminub_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { pminub_native_sse2(dst, src) };
            return;
        }
    }
    pminub_scalar(dst, src);
}

#[inline]
pub fn pminub_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { pminub_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                pminub_native_sse2(&mut dst[0], &src[0]);
                pminub_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    pminub_scalar(&mut dst[0], &src[0]);
    pminub_scalar(&mut dst[1], &src[1]);
}

// --- PMAXUB ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn pmaxub_native_sse2(dst: &mut Xmm, src: &Xmm) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let b = _mm_loadu_si128(src.as_ptr() as *const __m128i);
    let c = _mm_max_epu8(a, b);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
pub unsafe fn pmaxub_native_avx2(dst: &mut Ymm, src: &Ymm) {
    use std::arch::x86_64::*;
    let a = _mm256_loadu_si256(dst.as_ptr() as *const __m256i);
    let b = _mm256_loadu_si256(src.as_ptr() as *const __m256i);
    let c = _mm256_max_epu8(a, b);
    _mm256_storeu_si256(dst.as_mut_ptr() as *mut __m256i, c);
}

#[inline]
pub fn pmaxub_scalar(dst: &mut Xmm, src: &Xmm) {
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u8; 16]) };
    let s = unsafe { &*(src as *const Xmm as *const [u8; 16]) };
    for i in 0..16 { d[i] = d[i].max(s[i]); }
}

#[inline]
pub fn pmaxub_xmm(dst: &mut Xmm, src: &Xmm) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 {
            unsafe { pmaxub_native_sse2(dst, src) };
            return;
        }
    }
    pmaxub_scalar(dst, src);
}

#[inline]
pub fn pmaxub_ymm(dst: &mut Ymm, src: &Ymm) {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = SimdCapabilities::get();
        if caps.avx2 {
            unsafe { pmaxub_native_avx2(dst, src) };
            return;
        }
        if caps.sse2 {
            unsafe {
                pmaxub_native_sse2(&mut dst[0], &src[0]);
                pmaxub_native_sse2(&mut dst[1], &src[1]);
            }
            return;
        }
    }
    pmaxub_scalar(&mut dst[0], &src[0]);
    pmaxub_scalar(&mut dst[1], &src[1]);
}

// ============================================================================
// SHIFT OPERATIONS (PSLLW, PSLLD, PSLLQ, PSRLW, PSRLD, PSRLQ, etc.)
// ============================================================================

// --- PSLLW (shift left logical words) ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn psllw_native_sse2(dst: &mut Xmm, count: u8) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let cnt = _mm_cvtsi64_si128(count as i64);
    let c = _mm_sll_epi16(a, cnt);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[inline]
pub fn psllw_scalar(dst: &mut Xmm, count: u8) {
    if count >= 16 {
        dst[0] = 0;
        dst[1] = 0;
        return;
    }
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u16; 8]) };
    for i in 0..8 { d[i] <<= count; }
}

#[inline]
pub fn psllw_xmm(dst: &mut Xmm, count: u8) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 && count < 16 {
            unsafe { psllw_native_sse2(dst, count) };
            return;
        }
    }
    psllw_scalar(dst, count);
}

// --- PSLLD (shift left logical dwords) ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn pslld_native_sse2(dst: &mut Xmm, count: u8) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let cnt = _mm_cvtsi64_si128(count as i64);
    let c = _mm_sll_epi32(a, cnt);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[inline]
pub fn pslld_scalar(dst: &mut Xmm, count: u8) {
    if count >= 32 {
        dst[0] = 0;
        dst[1] = 0;
        return;
    }
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u32; 4]) };
    for i in 0..4 { d[i] <<= count; }
}

#[inline]
pub fn pslld_xmm(dst: &mut Xmm, count: u8) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 && count < 32 {
            unsafe { pslld_native_sse2(dst, count) };
            return;
        }
    }
    pslld_scalar(dst, count);
}

// --- PSLLQ (shift left logical qwords) ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn psllq_native_sse2(dst: &mut Xmm, count: u8) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let cnt = _mm_cvtsi64_si128(count as i64);
    let c = _mm_sll_epi64(a, cnt);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[inline]
pub fn psllq_scalar(dst: &mut Xmm, count: u8) {
    if count >= 64 {
        dst[0] = 0;
        dst[1] = 0;
        return;
    }
    dst[0] <<= count;
    dst[1] <<= count;
}

#[inline]
pub fn psllq_xmm(dst: &mut Xmm, count: u8) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 && count < 64 {
            unsafe { psllq_native_sse2(dst, count) };
            return;
        }
    }
    psllq_scalar(dst, count);
}

// --- PSRLW (shift right logical words) ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn psrlw_native_sse2(dst: &mut Xmm, count: u8) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let cnt = _mm_cvtsi64_si128(count as i64);
    let c = _mm_srl_epi16(a, cnt);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[inline]
pub fn psrlw_scalar(dst: &mut Xmm, count: u8) {
    if count >= 16 {
        dst[0] = 0;
        dst[1] = 0;
        return;
    }
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u16; 8]) };
    for i in 0..8 { d[i] >>= count; }
}

#[inline]
pub fn psrlw_xmm(dst: &mut Xmm, count: u8) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 && count < 16 {
            unsafe { psrlw_native_sse2(dst, count) };
            return;
        }
    }
    psrlw_scalar(dst, count);
}

// --- PSRLD (shift right logical dwords) ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn psrld_native_sse2(dst: &mut Xmm, count: u8) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let cnt = _mm_cvtsi64_si128(count as i64);
    let c = _mm_srl_epi32(a, cnt);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[inline]
pub fn psrld_scalar(dst: &mut Xmm, count: u8) {
    if count >= 32 {
        dst[0] = 0;
        dst[1] = 0;
        return;
    }
    let d = unsafe { &mut *(dst as *mut Xmm as *mut [u32; 4]) };
    for i in 0..4 { d[i] >>= count; }
}

#[inline]
pub fn psrld_xmm(dst: &mut Xmm, count: u8) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 && count < 32 {
            unsafe { psrld_native_sse2(dst, count) };
            return;
        }
    }
    psrld_scalar(dst, count);
}

// --- PSRLQ (shift right logical qwords) ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
#[inline]
pub unsafe fn psrlq_native_sse2(dst: &mut Xmm, count: u8) {
    use std::arch::x86_64::*;
    let a = _mm_loadu_si128(dst.as_ptr() as *const __m128i);
    let cnt = _mm_cvtsi64_si128(count as i64);
    let c = _mm_srl_epi64(a, cnt);
    _mm_storeu_si128(dst.as_mut_ptr() as *mut __m128i, c);
}

#[inline]
pub fn psrlq_scalar(dst: &mut Xmm, count: u8) {
    if count >= 64 {
        dst[0] = 0;
        dst[1] = 0;
        return;
    }
    dst[0] >>= count;
    dst[1] >>= count;
}

#[inline]
pub fn psrlq_xmm(dst: &mut Xmm, count: u8) {
    #[cfg(target_arch = "x86_64")]
    {
        if SimdCapabilities::get().sse2 && count < 64 {
            unsafe { psrlq_native_sse2(dst, count) };
            return;
        }
    }
    psrlq_scalar(dst, count);
}

// ============================================================================
// TESTS - All SIMD variants tested on available host features
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Test helpers
    // ========================================================================

    fn make_f32_xmm(a: f32, b: f32, c: f32, d: f32) -> Xmm {
        [
            a.to_bits() as u64 | ((b.to_bits() as u64) << 32),
            c.to_bits() as u64 | ((d.to_bits() as u64) << 32),
        ]
    }

    fn make_f64_xmm(a: f64, b: f64) -> Xmm {
        [a.to_bits(), b.to_bits()]
    }

    fn read_f32_xmm(xmm: &Xmm) -> [f32; 4] {
        unsafe { *(xmm.as_ptr() as *const [f32; 4]) }
    }

    fn read_f64_xmm(xmm: &Xmm) -> [f64; 2] {
        unsafe { *(xmm.as_ptr() as *const [f64; 2]) }
    }

    fn xmm_eq(a: &Xmm, b: &Xmm) -> bool {
        a[0] == b[0] && a[1] == b[1]
    }

    fn ymm_eq(a: &Ymm, b: &Ymm) -> bool {
        xmm_eq(&a[0], &b[0]) && xmm_eq(&a[1], &b[1])
    }

    // ========================================================================
    // Host capability detection test
    // ========================================================================

    #[test]
    fn test_host_capabilities() {
        let caps = SimdCapabilities::get();
        println!("Host SIMD capabilities:");
        println!("  SSE2:     {}", caps.sse2);
        println!("  SSE3:     {}", caps.sse3);
        println!("  SSSE3:    {}", caps.ssse3);
        println!("  SSE4.1:   {}", caps.sse4_1);
        println!("  SSE4.2:   {}", caps.sse4_2);
        println!("  AVX:      {}", caps.avx);
        println!("  AVX2:     {}", caps.avx2);
        println!("  FMA:      {}", caps.fma);
        println!("  AVX-512F: {}", caps.avx512f);
        println!("  AVX-512BW:{}", caps.avx512bw);
        println!("  AVX-512VL:{}", caps.avx512vl);

        #[cfg(target_arch = "x86_64")]
        assert!(caps.sse2, "x86_64 should have SSE2");
    }

    // ========================================================================
    // ADDPS - All variants
    // ========================================================================

    #[test]
    fn test_addps_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = make_f32_xmm(5.0, 6.0, 7.0, 8.0);

        // Scalar reference
        let mut scalar_dst = make_f32_xmm(1.0, 2.0, 3.0, 4.0);
        addps_scalar(&mut scalar_dst, &src);
        let expected = read_f32_xmm(&scalar_dst);
        assert_eq!(expected, [6.0, 8.0, 10.0, 12.0]);

        #[cfg(target_arch = "x86_64")]
        {
            // SSE native (XMM)
            if caps.sse2 {
                let mut sse_dst = make_f32_xmm(1.0, 2.0, 3.0, 4.0);
                unsafe { addps_native_sse(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE ADDPS mismatch");
                println!("  ADDPS SSE: PASS");
            }

            // AVX native (YMM - tests the 256-bit version)
            if caps.avx {
                let src_ymm: Ymm = [src, make_f32_xmm(9.0, 10.0, 11.0, 12.0)];
                let mut scalar_ymm: Ymm = [make_f32_xmm(1.0, 2.0, 3.0, 4.0), make_f32_xmm(1.0, 2.0, 3.0, 4.0)];
                addps_scalar(&mut scalar_ymm[0], &src_ymm[0]);
                addps_scalar(&mut scalar_ymm[1], &src_ymm[1]);

                let mut avx_dst: Ymm = [make_f32_xmm(1.0, 2.0, 3.0, 4.0), make_f32_xmm(1.0, 2.0, 3.0, 4.0)];
                unsafe { addps_native_avx(&mut avx_dst, &src_ymm) };
                assert!(ymm_eq(&avx_dst, &scalar_ymm), "AVX YMM ADDPS mismatch");
                println!("  ADDPS AVX YMM: PASS");
            }
        }

        // Dispatch function
        let mut dispatch_dst = make_f32_xmm(1.0, 2.0, 3.0, 4.0);
        addps_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "ADDPS dispatch mismatch");
        println!("  ADDPS dispatch: PASS");
    }

    #[test]
    fn test_addps_ymm_dispatch() {
        let src: Ymm = [make_f32_xmm(1.0, 2.0, 3.0, 4.0), make_f32_xmm(5.0, 6.0, 7.0, 8.0)];

        // Scalar reference
        let mut scalar_dst: Ymm = [make_f32_xmm(10.0, 20.0, 30.0, 40.0), make_f32_xmm(50.0, 60.0, 70.0, 80.0)];
        addps_scalar(&mut scalar_dst[0], &src[0]);
        addps_scalar(&mut scalar_dst[1], &src[1]);

        // Dispatch
        let mut dispatch_dst: Ymm = [make_f32_xmm(10.0, 20.0, 30.0, 40.0), make_f32_xmm(50.0, 60.0, 70.0, 80.0)];
        addps_ymm(&mut dispatch_dst, &src);
        assert!(ymm_eq(&dispatch_dst, &scalar_dst), "ADDPS YMM dispatch mismatch");
        println!("  ADDPS YMM dispatch: PASS");
    }

    // ========================================================================
    // SUBPS - All variants
    // ========================================================================

    #[test]
    fn test_subps_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = make_f32_xmm(1.0, 2.0, 3.0, 4.0);

        // Scalar reference
        let mut scalar_dst = make_f32_xmm(10.0, 20.0, 30.0, 40.0);
        subps_scalar(&mut scalar_dst, &src);
        let expected = read_f32_xmm(&scalar_dst);
        assert_eq!(expected, [9.0, 18.0, 27.0, 36.0]);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst = make_f32_xmm(10.0, 20.0, 30.0, 40.0);
                unsafe { subps_native_sse(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE SUBPS mismatch");
                println!("  SUBPS SSE: PASS");
            }

            if caps.avx {
                let src_ymm: Ymm = [src, make_f32_xmm(5.0, 6.0, 7.0, 8.0)];
                let mut scalar_ymm: Ymm = [make_f32_xmm(10.0, 20.0, 30.0, 40.0), make_f32_xmm(50.0, 60.0, 70.0, 80.0)];
                subps_scalar(&mut scalar_ymm[0], &src_ymm[0]);
                subps_scalar(&mut scalar_ymm[1], &src_ymm[1]);

                let mut avx_dst: Ymm = [make_f32_xmm(10.0, 20.0, 30.0, 40.0), make_f32_xmm(50.0, 60.0, 70.0, 80.0)];
                unsafe { subps_native_avx(&mut avx_dst, &src_ymm) };
                assert!(ymm_eq(&avx_dst, &scalar_ymm), "AVX YMM SUBPS mismatch");
                println!("  SUBPS AVX YMM: PASS");
            }
        }

        let mut dispatch_dst = make_f32_xmm(10.0, 20.0, 30.0, 40.0);
        subps_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "SUBPS dispatch mismatch");
        println!("  SUBPS dispatch: PASS");
    }

    // ========================================================================
    // MULPS - All variants
    // ========================================================================

    #[test]
    fn test_mulps_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = make_f32_xmm(2.0, 3.0, 4.0, 5.0);

        // Scalar reference
        let mut scalar_dst = make_f32_xmm(1.0, 2.0, 3.0, 4.0);
        mulps_scalar(&mut scalar_dst, &src);
        let expected = read_f32_xmm(&scalar_dst);
        assert_eq!(expected, [2.0, 6.0, 12.0, 20.0]);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst = make_f32_xmm(1.0, 2.0, 3.0, 4.0);
                unsafe { mulps_native_sse(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE MULPS mismatch");
                println!("  MULPS SSE: PASS");
            }

            if caps.avx {
                let src_ymm: Ymm = [src, make_f32_xmm(2.0, 2.0, 2.0, 2.0)];
                let mut scalar_ymm: Ymm = [make_f32_xmm(1.0, 2.0, 3.0, 4.0), make_f32_xmm(5.0, 6.0, 7.0, 8.0)];
                mulps_scalar(&mut scalar_ymm[0], &src_ymm[0]);
                mulps_scalar(&mut scalar_ymm[1], &src_ymm[1]);

                let mut avx_dst: Ymm = [make_f32_xmm(1.0, 2.0, 3.0, 4.0), make_f32_xmm(5.0, 6.0, 7.0, 8.0)];
                unsafe { mulps_native_avx(&mut avx_dst, &src_ymm) };
                assert!(ymm_eq(&avx_dst, &scalar_ymm), "AVX YMM MULPS mismatch");
                println!("  MULPS AVX YMM: PASS");
            }
        }

        let mut dispatch_dst = make_f32_xmm(1.0, 2.0, 3.0, 4.0);
        mulps_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "MULPS dispatch mismatch");
        println!("  MULPS dispatch: PASS");
    }

    // ========================================================================
    // DIVPS - All variants
    // ========================================================================

    #[test]
    fn test_divps_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = make_f32_xmm(2.0, 4.0, 5.0, 8.0);

        // Scalar reference
        let mut scalar_dst = make_f32_xmm(10.0, 20.0, 30.0, 40.0);
        divps_scalar(&mut scalar_dst, &src);
        let expected = read_f32_xmm(&scalar_dst);
        assert_eq!(expected, [5.0, 5.0, 6.0, 5.0]);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst = make_f32_xmm(10.0, 20.0, 30.0, 40.0);
                unsafe { divps_native_sse(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE DIVPS mismatch");
                println!("  DIVPS SSE: PASS");
            }

            if caps.avx {
                let src_ymm: Ymm = [src, make_f32_xmm(2.0, 5.0, 10.0, 4.0)];
                let mut scalar_ymm: Ymm = [make_f32_xmm(10.0, 20.0, 30.0, 40.0), make_f32_xmm(20.0, 25.0, 30.0, 40.0)];
                divps_scalar(&mut scalar_ymm[0], &src_ymm[0]);
                divps_scalar(&mut scalar_ymm[1], &src_ymm[1]);

                let mut avx_dst: Ymm = [make_f32_xmm(10.0, 20.0, 30.0, 40.0), make_f32_xmm(20.0, 25.0, 30.0, 40.0)];
                unsafe { divps_native_avx(&mut avx_dst, &src_ymm) };
                assert!(ymm_eq(&avx_dst, &scalar_ymm), "AVX YMM DIVPS mismatch");
                println!("  DIVPS AVX YMM: PASS");
            }
        }

        let mut dispatch_dst = make_f32_xmm(10.0, 20.0, 30.0, 40.0);
        divps_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "DIVPS dispatch mismatch");
        println!("  DIVPS dispatch: PASS");
    }

    // ========================================================================
    // ADDPD - All variants
    // ========================================================================

    #[test]
    fn test_addpd_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = make_f64_xmm(5.0, 6.0);

        // Scalar reference
        let mut scalar_dst = make_f64_xmm(1.0, 2.0);
        addpd_scalar(&mut scalar_dst, &src);
        let expected = read_f64_xmm(&scalar_dst);
        assert_eq!(expected, [6.0, 8.0]);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst = make_f64_xmm(1.0, 2.0);
                unsafe { addpd_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 ADDPD mismatch");
                println!("  ADDPD SSE2: PASS");
            }
        }

        let mut dispatch_dst = make_f64_xmm(1.0, 2.0);
        addpd_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "ADDPD dispatch mismatch");
        println!("  ADDPD dispatch: PASS");
    }

    // ========================================================================
    // SUBPD - All variants
    // ========================================================================

    #[test]
    fn test_subpd_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = make_f64_xmm(1.0, 2.0);

        // Scalar reference
        let mut scalar_dst = make_f64_xmm(10.0, 20.0);
        subpd_scalar(&mut scalar_dst, &src);
        let expected = read_f64_xmm(&scalar_dst);
        assert_eq!(expected, [9.0, 18.0]);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst = make_f64_xmm(10.0, 20.0);
                unsafe { subpd_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 SUBPD mismatch");
                println!("  SUBPD SSE2: PASS");
            }
        }

        let mut dispatch_dst = make_f64_xmm(10.0, 20.0);
        subpd_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "SUBPD dispatch mismatch");
        println!("  SUBPD dispatch: PASS");
    }

    // ========================================================================
    // MULPD - All variants
    // ========================================================================

    #[test]
    fn test_mulpd_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = make_f64_xmm(2.0, 3.0);

        // Scalar reference
        let mut scalar_dst = make_f64_xmm(5.0, 10.0);
        mulpd_scalar(&mut scalar_dst, &src);
        let expected = read_f64_xmm(&scalar_dst);
        assert_eq!(expected, [10.0, 30.0]);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst = make_f64_xmm(5.0, 10.0);
                unsafe { mulpd_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 MULPD mismatch");
                println!("  MULPD SSE2: PASS");
            }
        }

        let mut dispatch_dst = make_f64_xmm(5.0, 10.0);
        mulpd_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "MULPD dispatch mismatch");
        println!("  MULPD dispatch: PASS");
    }

    // ========================================================================
    // DIVPD - All variants
    // ========================================================================

    #[test]
    fn test_divpd_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = make_f64_xmm(2.0, 5.0);

        // Scalar reference
        let mut scalar_dst = make_f64_xmm(10.0, 25.0);
        divpd_scalar(&mut scalar_dst, &src);
        let expected = read_f64_xmm(&scalar_dst);
        assert_eq!(expected, [5.0, 5.0]);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst = make_f64_xmm(10.0, 25.0);
                unsafe { divpd_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 DIVPD mismatch");
                println!("  DIVPD SSE2: PASS");
            }
        }

        let mut dispatch_dst = make_f64_xmm(10.0, 25.0);
        divpd_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "DIVPD dispatch mismatch");
        println!("  DIVPD dispatch: PASS");
    }

    // ========================================================================
    // PADDB - All variants
    // ========================================================================

    #[test]
    fn test_paddb_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = [0x0101010101010101, 0x0202020202020202];

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0102030405060708, 0x090A0B0C0D0E0F10];
        paddb_scalar(&mut scalar_dst, &src);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0102030405060708, 0x090A0B0C0D0E0F10];
                unsafe { paddb_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PADDB mismatch");
                println!("  PADDB SSE2: PASS");
            }

            if caps.avx2 {
                // Test YMM variant
                let src_ymm: Ymm = [[0x0101010101010101, 0x0101010101010101],
                                    [0x0202020202020202, 0x0202020202020202]];
                let mut scalar_ymm: Ymm = [[0x0102030405060708, 0x090A0B0C0D0E0F10],
                                           [0x1112131415161718, 0x191A1B1C1D1E1F20]];
                paddb_scalar(&mut scalar_ymm[0], &src_ymm[0]);
                paddb_scalar(&mut scalar_ymm[1], &src_ymm[1]);

                let mut avx2_ymm: Ymm = [[0x0102030405060708, 0x090A0B0C0D0E0F10],
                                         [0x1112131415161718, 0x191A1B1C1D1E1F20]];
                unsafe { paddb_native_avx2(&mut avx2_ymm, &src_ymm) };
                assert!(ymm_eq(&avx2_ymm, &scalar_ymm), "AVX2 PADDB YMM mismatch");
                println!("  PADDB AVX2 YMM: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0102030405060708, 0x090A0B0C0D0E0F10];
        paddb_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PADDB dispatch mismatch");
        println!("  PADDB dispatch: PASS");
    }

    // ========================================================================
    // PADDW - All variants
    // ========================================================================

    #[test]
    fn test_paddw_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = [0x0001000100010001, 0x0002000200020002];

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0102030405060708, 0x090A0B0C0D0E0F10];
        paddw_scalar(&mut scalar_dst, &src);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0102030405060708, 0x090A0B0C0D0E0F10];
                unsafe { paddw_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PADDW mismatch");
                println!("  PADDW SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0102030405060708, 0x090A0B0C0D0E0F10];
        paddw_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PADDW dispatch mismatch");
        println!("  PADDW dispatch: PASS");
    }

    // ========================================================================
    // PADDD - All variants
    // ========================================================================

    #[test]
    fn test_paddd_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = [0x0000000100000001, 0x0000000200000002];

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0000001000000020, 0x0000003000000040];
        paddd_scalar(&mut scalar_dst, &src);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0000001000000020, 0x0000003000000040];
                unsafe { paddd_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PADDD mismatch");
                println!("  PADDD SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0000001000000020, 0x0000003000000040];
        paddd_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PADDD dispatch mismatch");
        println!("  PADDD dispatch: PASS");
    }

    // ========================================================================
    // PADDQ - All variants
    // ========================================================================

    #[test]
    fn test_paddq_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = [1, 2];

        // Scalar reference
        let mut scalar_dst: Xmm = [100, 200];
        paddq_scalar(&mut scalar_dst, &src);
        assert_eq!(scalar_dst, [101, 202]);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [100, 200];
                unsafe { paddq_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PADDQ mismatch");
                println!("  PADDQ SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [100, 200];
        paddq_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PADDQ dispatch mismatch");
        println!("  PADDQ dispatch: PASS");
    }

    // ========================================================================
    // PSUBB - All variants
    // ========================================================================

    #[test]
    fn test_psubb_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = [0x0101010101010101, 0x0101010101010101];

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0203040506070809, 0x0A0B0C0D0E0F1011];
        psubb_scalar(&mut scalar_dst, &src);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0203040506070809, 0x0A0B0C0D0E0F1011];
                unsafe { psubb_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PSUBB mismatch");
                println!("  PSUBB SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0203040506070809, 0x0A0B0C0D0E0F1011];
        psubb_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PSUBB dispatch mismatch");
        println!("  PSUBB dispatch: PASS");
    }

    // ========================================================================
    // PAND - All variants
    // ========================================================================

    #[test]
    fn test_pand_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = [0xF0F0F0F0F0F0F0F0, 0x0F0F0F0F0F0F0F0F];

        // Scalar reference
        let mut scalar_dst: Xmm = [0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF];
        pand_scalar(&mut scalar_dst, &src);
        assert_eq!(scalar_dst, [0xF0F0F0F0F0F0F0F0, 0x0F0F0F0F0F0F0F0F]);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF];
                unsafe { pand_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PAND mismatch");
                println!("  PAND SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF];
        pand_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PAND dispatch mismatch");
        println!("  PAND dispatch: PASS");
    }

    // ========================================================================
    // POR - All variants
    // ========================================================================

    #[test]
    fn test_por_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = [0xF0F0F0F0F0F0F0F0, 0x0F0F0F0F0F0F0F0F];

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0F0F0F0F0F0F0F0F, 0xF0F0F0F0F0F0F0F0];
        por_scalar(&mut scalar_dst, &src);
        assert_eq!(scalar_dst, [0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF]);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0F0F0F0F0F0F0F0F, 0xF0F0F0F0F0F0F0F0];
                unsafe { por_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 POR mismatch");
                println!("  POR SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0F0F0F0F0F0F0F0F, 0xF0F0F0F0F0F0F0F0];
        por_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "POR dispatch mismatch");
        println!("  POR dispatch: PASS");
    }

    // ========================================================================
    // PXOR - All variants
    // ========================================================================

    #[test]
    fn test_pxor_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = [0xFFFFFFFFFFFFFFFF, 0x5555555555555555];

        // Scalar reference
        let mut scalar_dst: Xmm = [0xFFFFFFFFFFFFFFFF, 0xAAAAAAAAAAAAAAAA];
        pxor_scalar(&mut scalar_dst, &src);
        assert_eq!(scalar_dst, [0, 0xFFFFFFFFFFFFFFFF]);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0xFFFFFFFFFFFFFFFF, 0xAAAAAAAAAAAAAAAA];
                unsafe { pxor_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PXOR mismatch");
                println!("  PXOR SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0xFFFFFFFFFFFFFFFF, 0xAAAAAAAAAAAAAAAA];
        pxor_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PXOR dispatch mismatch");
        println!("  PXOR dispatch: PASS");
    }

    // ========================================================================
    // PANDN - All variants
    // ========================================================================

    #[test]
    fn test_pandn_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = [0xFFFFFFFFFFFFFFFF, 0xAAAAAAAAAAAAAAAA];

        // Scalar reference: result = ~dst & src
        let mut scalar_dst: Xmm = [0xF0F0F0F0F0F0F0F0, 0x0F0F0F0F0F0F0F0F];
        pandn_scalar(&mut scalar_dst, &src);
        // ~0xF0F0F0F0F0F0F0F0 = 0x0F0F0F0F0F0F0F0F
        // 0x0F0F0F0F0F0F0F0F & 0xFFFFFFFFFFFFFFFF = 0x0F0F0F0F0F0F0F0F
        assert_eq!(scalar_dst[0], 0x0F0F0F0F0F0F0F0F);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0xF0F0F0F0F0F0F0F0, 0x0F0F0F0F0F0F0F0F];
                unsafe { pandn_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PANDN mismatch");
                println!("  PANDN SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0xF0F0F0F0F0F0F0F0, 0x0F0F0F0F0F0F0F0F];
        pandn_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PANDN dispatch mismatch");
        println!("  PANDN dispatch: PASS");
    }

    // ========================================================================
    // PCMPEQB - All variants
    // ========================================================================

    #[test]
    fn test_pcmpeqb_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = [0x0102030401020305, 0x0506070805060708];

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0102030401020304, 0x0506070805060708];
        pcmpeqb_scalar(&mut scalar_dst, &src);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0102030401020304, 0x0506070805060708];
                unsafe { pcmpeqb_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PCMPEQB mismatch");
                println!("  PCMPEQB SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0102030401020304, 0x0506070805060708];
        pcmpeqb_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PCMPEQB dispatch mismatch");
        println!("  PCMPEQB dispatch: PASS");
    }

    // ========================================================================
    // PCMPEQD - All variants
    // ========================================================================

    #[test]
    fn test_pcmpeqd_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = [0x0000000100000002, 0x0000000300000004];

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0000000100000003, 0x0000000300000004];
        pcmpeqd_scalar(&mut scalar_dst, &src);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0000000100000003, 0x0000000300000004];
                unsafe { pcmpeqd_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PCMPEQD mismatch");
                println!("  PCMPEQD SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0000000100000003, 0x0000000300000004];
        pcmpeqd_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PCMPEQD dispatch mismatch");
        println!("  PCMPEQD dispatch: PASS");
    }

    // ========================================================================
    // PSHUFB - All variants (SSSE3)
    // ========================================================================

    #[test]
    fn test_pshufb_all_variants() {
        let caps = SimdCapabilities::get();
        // Mask: 0x80 means zero, otherwise index into dst
        let mask: Xmm = [0x0001020380808080, 0x0F0E0D0C0B0A0908];

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0706050403020100, 0x0F0E0D0C0B0A0908];
        pshufb_scalar(&mut scalar_dst, &mask);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.ssse3 {
                let mut ssse3_dst: Xmm = [0x0706050403020100, 0x0F0E0D0C0B0A0908];
                unsafe { pshufb_native_ssse3(&mut ssse3_dst, &mask) };
                assert!(xmm_eq(&ssse3_dst, &scalar_dst), "SSSE3 PSHUFB mismatch");
                println!("  PSHUFB SSSE3: PASS");
            }

            if caps.avx2 {
                // YMM variant
                let mask_ymm: Ymm = [[0x0001020380808080, 0x0F0E0D0C0B0A0908],
                                     [0x0302010003020100, 0x0706050407060504]];
                let mut scalar_ymm: Ymm = [[0x0706050403020100, 0x0F0E0D0C0B0A0908],
                                           [0x1716151413121110, 0x1F1E1D1C1B1A1918]];
                pshufb_scalar(&mut scalar_ymm[0], &mask_ymm[0]);
                pshufb_scalar(&mut scalar_ymm[1], &mask_ymm[1]);

                let mut avx2_ymm: Ymm = [[0x0706050403020100, 0x0F0E0D0C0B0A0908],
                                         [0x1716151413121110, 0x1F1E1D1C1B1A1918]];
                unsafe { pshufb_native_avx2(&mut avx2_ymm, &mask_ymm) };
                assert!(ymm_eq(&avx2_ymm, &scalar_ymm), "AVX2 PSHUFB YMM mismatch");
                println!("  PSHUFB AVX2 YMM: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0706050403020100, 0x0F0E0D0C0B0A0908];
        pshufb_xmm(&mut dispatch_dst, &mask);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PSHUFB dispatch mismatch");
        println!("  PSHUFB dispatch: PASS");
    }

    // ========================================================================
    // PMULLW - All variants
    // ========================================================================

    #[test]
    fn test_pmullw_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = [0x0002000200020002, 0x0003000300030003];

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0004000300020001, 0x0008000700060005];
        pmullw_scalar(&mut scalar_dst, &src);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0004000300020001, 0x0008000700060005];
                unsafe { pmullw_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PMULLW mismatch");
                println!("  PMULLW SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0004000300020001, 0x0008000700060005];
        pmullw_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PMULLW dispatch mismatch");
        println!("  PMULLW dispatch: PASS");
    }

    // ========================================================================
    // PMULLD - All variants (SSE4.1)
    // ========================================================================

    #[test]
    fn test_pmulld_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = [0x0000000200000003, 0x0000000400000005];

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0000000500000004, 0x0000000300000002];
        pmulld_scalar(&mut scalar_dst, &src);
        // 5*2=10, 4*3=12, 3*4=12, 2*5=10

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse4_1 {
                let mut sse41_dst: Xmm = [0x0000000500000004, 0x0000000300000002];
                unsafe { pmulld_native_sse41(&mut sse41_dst, &src) };
                assert!(xmm_eq(&sse41_dst, &scalar_dst), "SSE4.1 PMULLD mismatch");
                println!("  PMULLD SSE4.1: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0000000500000004, 0x0000000300000002];
        pmulld_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PMULLD dispatch mismatch");
        println!("  PMULLD dispatch: PASS");
    }

    // ========================================================================
    // PMADDWD - All variants
    // ========================================================================

    #[test]
    fn test_pmaddwd_all_variants() {
        let caps = SimdCapabilities::get();
        // src: [1, 2, 3, 4, 5, 6, 7, 8]
        let src: Xmm = [0x0002000100040003, 0x0006000500080007];
        // dst: [1, 1, 1, 1, 1, 1, 1, 1]
        let base: Xmm = [0x0001000100010001, 0x0001000100010001];

        // Scalar reference
        let mut scalar_dst = base;
        pmaddwd_scalar(&mut scalar_dst, &src);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst = base;
                unsafe { pmaddwd_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PMADDWD mismatch");
                println!("  PMADDWD SSE2: PASS");
            }
        }

        let mut dispatch_dst = base;
        pmaddwd_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PMADDWD dispatch mismatch");
        println!("  PMADDWD dispatch: PASS");
    }

    // ========================================================================
    // PMINUB - All variants
    // ========================================================================

    #[test]
    fn test_pminub_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = [0x0506070801020304, 0x0D0E0F10090A0B0C];

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0102030405060708, 0x090A0B0C0D0E0F10];
        pminub_scalar(&mut scalar_dst, &src);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0102030405060708, 0x090A0B0C0D0E0F10];
                unsafe { pminub_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PMINUB mismatch");
                println!("  PMINUB SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0102030405060708, 0x090A0B0C0D0E0F10];
        pminub_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PMINUB dispatch mismatch");
        println!("  PMINUB dispatch: PASS");
    }

    // ========================================================================
    // PMAXUB - All variants
    // ========================================================================

    #[test]
    fn test_pmaxub_all_variants() {
        let caps = SimdCapabilities::get();
        let src: Xmm = [0x0506070801020304, 0x0D0E0F10090A0B0C];

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0102030405060708, 0x090A0B0C0D0E0F10];
        pmaxub_scalar(&mut scalar_dst, &src);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0102030405060708, 0x090A0B0C0D0E0F10];
                unsafe { pmaxub_native_sse2(&mut sse_dst, &src) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PMAXUB mismatch");
                println!("  PMAXUB SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0102030405060708, 0x090A0B0C0D0E0F10];
        pmaxub_xmm(&mut dispatch_dst, &src);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PMAXUB dispatch mismatch");
        println!("  PMAXUB dispatch: PASS");
    }

    // ========================================================================
    // PSLLW - All variants
    // ========================================================================

    #[test]
    fn test_psllw_all_variants() {
        let caps = SimdCapabilities::get();

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0001000200030004, 0x0005000600070008];
        psllw_scalar(&mut scalar_dst, 4);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0001000200030004, 0x0005000600070008];
                unsafe { psllw_native_sse2(&mut sse_dst, 4) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PSLLW mismatch");
                println!("  PSLLW SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0001000200030004, 0x0005000600070008];
        psllw_xmm(&mut dispatch_dst, 4);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PSLLW dispatch mismatch");
        println!("  PSLLW dispatch: PASS");

        // Test shift >= 16 clears
        let mut clear_dst: Xmm = [0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF];
        psllw_xmm(&mut clear_dst, 16);
        assert_eq!(clear_dst, [0, 0], "PSLLW shift>=16 should clear");
    }

    // ========================================================================
    // PSLLD - All variants
    // ========================================================================

    #[test]
    fn test_pslld_all_variants() {
        let caps = SimdCapabilities::get();

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0000000100000002, 0x0000000300000004];
        pslld_scalar(&mut scalar_dst, 8);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0000000100000002, 0x0000000300000004];
                unsafe { pslld_native_sse2(&mut sse_dst, 8) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PSLLD mismatch");
                println!("  PSLLD SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0000000100000002, 0x0000000300000004];
        pslld_xmm(&mut dispatch_dst, 8);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PSLLD dispatch mismatch");
        println!("  PSLLD dispatch: PASS");
    }

    // ========================================================================
    // PSLLQ - All variants
    // ========================================================================

    #[test]
    fn test_psllq_all_variants() {
        let caps = SimdCapabilities::get();

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0000000000000001, 0x0000000000000002];
        psllq_scalar(&mut scalar_dst, 32);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0000000000000001, 0x0000000000000002];
                unsafe { psllq_native_sse2(&mut sse_dst, 32) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PSLLQ mismatch");
                println!("  PSLLQ SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0000000000000001, 0x0000000000000002];
        psllq_xmm(&mut dispatch_dst, 32);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PSLLQ dispatch mismatch");
        println!("  PSLLQ dispatch: PASS");
    }

    // ========================================================================
    // PSRLW - All variants
    // ========================================================================

    #[test]
    fn test_psrlw_all_variants() {
        let caps = SimdCapabilities::get();

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0010002000300040, 0x0050006000700080];
        psrlw_scalar(&mut scalar_dst, 4);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0010002000300040, 0x0050006000700080];
                unsafe { psrlw_native_sse2(&mut sse_dst, 4) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PSRLW mismatch");
                println!("  PSRLW SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0010002000300040, 0x0050006000700080];
        psrlw_xmm(&mut dispatch_dst, 4);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PSRLW dispatch mismatch");
        println!("  PSRLW dispatch: PASS");
    }

    // ========================================================================
    // PSRLD - All variants
    // ========================================================================

    #[test]
    fn test_psrld_all_variants() {
        let caps = SimdCapabilities::get();

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0000010000000200, 0x0000030000000400];
        psrld_scalar(&mut scalar_dst, 8);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0000010000000200, 0x0000030000000400];
                unsafe { psrld_native_sse2(&mut sse_dst, 8) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PSRLD mismatch");
                println!("  PSRLD SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0000010000000200, 0x0000030000000400];
        psrld_xmm(&mut dispatch_dst, 8);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PSRLD dispatch mismatch");
        println!("  PSRLD dispatch: PASS");
    }

    // ========================================================================
    // PSRLQ - All variants
    // ========================================================================

    #[test]
    fn test_psrlq_all_variants() {
        let caps = SimdCapabilities::get();

        // Scalar reference
        let mut scalar_dst: Xmm = [0x0000000100000000, 0x0000000200000000];
        psrlq_scalar(&mut scalar_dst, 32);

        #[cfg(target_arch = "x86_64")]
        {
            if caps.sse2 {
                let mut sse_dst: Xmm = [0x0000000100000000, 0x0000000200000000];
                unsafe { psrlq_native_sse2(&mut sse_dst, 32) };
                assert!(xmm_eq(&sse_dst, &scalar_dst), "SSE2 PSRLQ mismatch");
                println!("  PSRLQ SSE2: PASS");
            }
        }

        let mut dispatch_dst: Xmm = [0x0000000100000000, 0x0000000200000000];
        psrlq_xmm(&mut dispatch_dst, 32);
        assert!(xmm_eq(&dispatch_dst, &scalar_dst), "PSRLQ dispatch mismatch");
        println!("  PSRLQ dispatch: PASS");
    }

    // ========================================================================
    // Comprehensive summary test
    // ========================================================================

    #[test]
    fn test_simd_variant_summary() {
        let caps = SimdCapabilities::get();
        println!("\n=== SIMD Variant Test Summary ===");
        println!("Host capabilities:");
        println!("  SSE2:     {}", caps.sse2);
        println!("  SSE3:     {}", caps.sse3);
        println!("  SSSE3:    {}", caps.ssse3);
        println!("  SSE4.1:   {}", caps.sse4_1);
        println!("  SSE4.2:   {}", caps.sse4_2);
        println!("  AVX:      {}", caps.avx);
        println!("  AVX2:     {}", caps.avx2);
        println!("  FMA:      {}", caps.fma);
        println!("  AVX-512F: {}", caps.avx512f);

        let mut tested_variants = Vec::new();
        tested_variants.push("scalar");
        if caps.sse2 { tested_variants.push("SSE2"); }
        if caps.ssse3 { tested_variants.push("SSSE3"); }
        if caps.sse4_1 { tested_variants.push("SSE4.1"); }
        if caps.avx { tested_variants.push("AVX"); }
        if caps.avx2 { tested_variants.push("AVX2"); }

        println!("\nTested variants: {}", tested_variants.join(", "));
        println!("=================================\n");
    }
}
