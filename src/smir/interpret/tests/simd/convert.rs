//! simd::convert tests

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

#[test]
fn test_vnarrowshiftsat_wh_shift_round_sat() {
    // word->half, signed, shift=4, round, saturate signed.
    // src word = 0x0000_00FF = 255. round bias = 1<<3 = 8. (255+8)>>4 = 16.
    let v0 = [0x0000_00FFu64 | (0x0000_00FFu64 << 32); 16];
    let v1 = [0x0000_00FFu64 | (0x0000_00FFu64 << 32); 16];
    let out = run_narrow_shift_sat(v0, v1, 4, VecElementType::I32, true, true, 1);
    assert_eq!(out, [0x0010_0010_0010_0010u64; 16]); // 16 per half
}
