//! FPMulAddH: exact binary16 inputs, one binary32 rounding and guest status.
use super::{Fpscr, vcvt_f32_f16_bits, vfma_f32};
fn operand(raw: u16, control: u32) -> f32 {
    // Data FPUnpack flushes half inputs via FZ16 without raising IDC. The
    // raw IEEE widening map preserves NaN type/payload for FPMulAdd processing
    // and is exact for every finite half value. AHP does not apply here.
    let bits = if control & (1 << 19) != 0 && raw & 0x7c00 == 0 {
        raw & 0x8000
    } else {
        raw
    };
    vcvt_f32_f16_bits(bits)
}
/// FPMulAddH with exact half operands and one final binary32 rounding.
pub fn vfmal_f32_f16_bits(accumulator: u32, first: u16, second: u16, fpscr: &mut Fpscr) -> u32 {
    let a = operand(first, fpscr.bits());
    let b = operand(second, fpscr.bits());
    // Widened finite half subnormals are normal binary32 values. FZ32 therefore
    // acts only on the binary32 addend/output; signaling NaNs remain signaling
    // until the shared fused evaluator applies the architectural priority.
    vfma_f32(f32::from_bits(accumulator), a, b, fpscr).to_bits()
}
/// Subtract the exact half product, negating the first input before NaN handling.
pub fn vfmsl_f32_f16_bits(accumulator: u32, first: u16, second: u16, fpscr: &mut Fpscr) -> u32 {
    vfmal_f32_f16_bits(accumulator, first ^ 0x8000, second, fpscr)
}
