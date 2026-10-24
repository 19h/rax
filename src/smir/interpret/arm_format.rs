//! Data-processing binary16 observes FZ16, ignores AHP, and never raises IDC
//! merely because a half-precision input was flushed (FPUnpackBase).
pub(super) fn half_control(control: u32) -> u32 {
    (control & !((1 << 24) | (1 << 26))) | (((control >> 19) & 1) << 24)
}
