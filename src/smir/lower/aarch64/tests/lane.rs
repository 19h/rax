//! Native integer lane arithmetic with independent boundary expectations.

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{VLaneOp, VecElementType};

use super::{lower_ops, run_aarch64_code_with_regs_and_simd, v};

#[test]
fn lowers_vlane_integer_runtime() {
    fn apply_vlane(
        a_low: u64,
        a_high: u64,
        b_low: u64,
        b_high: u64,
        elem_bytes: usize,
        lanes: usize,
        op: VLaneOp,
        signed: bool,
    ) -> (u64, u64) {
        fn read_lane(bytes: &[u8; 16], offset: usize, len: usize) -> u64 {
            let mut word = [0u8; 8];
            word[..len].copy_from_slice(&bytes[offset..offset + len]);
            u64::from_le_bytes(word)
        }

        let mut a = [0u8; 16];
        let mut b = [0u8; 16];
        let mut out = [0u8; 16];
        a[..8].copy_from_slice(&a_low.to_le_bytes());
        a[8..].copy_from_slice(&a_high.to_le_bytes());
        b[..8].copy_from_slice(&b_low.to_le_bytes());
        b[8..].copy_from_slice(&b_high.to_le_bytes());

        let elem_bits = elem_bytes * 8;
        let mask = if elem_bits == 64 {
            u64::MAX
        } else {
            (1u64 << elem_bits) - 1
        };
        let sx = |value: u64| -> i128 {
            if elem_bits == 64 {
                value as i64 as i128
            } else {
                let shift = 64 - elem_bits;
                ((value << shift) as i64 >> shift) as i128
            }
        };
        let smin = if elem_bits == 64 {
            i64::MIN as i128
        } else {
            -(1i128 << (elem_bits - 1))
        };
        let smax = if elem_bits == 64 {
            i64::MAX as i128
        } else {
            (1i128 << (elem_bits - 1)) - 1
        };

        for lane in 0..lanes {
            let off = lane * elem_bytes;
            let av = read_lane(&a, off, elem_bytes);
            let bv = read_lane(&b, off, elem_bytes);
            let result = match op {
                VLaneOp::Add => av.wrapping_add(bv),
                VLaneOp::Sub => av.wrapping_sub(bv),
                VLaneOp::Mul => av.wrapping_mul(bv),
                VLaneOp::Min if signed => sx(av).min(sx(bv)) as u64,
                VLaneOp::Min => (av & mask).min(bv & mask),
                VLaneOp::Max if signed => sx(av).max(sx(bv)) as u64,
                VLaneOp::Max => (av & mask).max(bv & mask),
                VLaneOp::And => av & bv,
                VLaneOp::Or => av | bv,
                VLaneOp::Xor => av ^ bv,
                VLaneOp::AndNot => av & !bv,
                VLaneOp::OrNot => av | !bv,
                VLaneOp::Not => !av,
                VLaneOp::AddSat if signed => (sx(av) + sx(bv)).clamp(smin, smax) as u64,
                VLaneOp::AddSat => {
                    ((av & mask) as u128 + (bv & mask) as u128).min(mask as u128) as u64
                }
                VLaneOp::SubSat if signed => (sx(av) - sx(bv)).clamp(smin, smax) as u64,
                VLaneOp::SubSat => (av & mask).saturating_sub(bv & mask),
                VLaneOp::Avg if signed => ((sx(av) + sx(bv)) >> 1) as u64,
                VLaneOp::Avg => (((av & mask) as u128 + (bv & mask) as u128) >> 1) as u64,
                VLaneOp::AvgRnd if signed => ((sx(av) + sx(bv) + 1) >> 1) as u64,
                VLaneOp::AvgRnd => (((av & mask) as u128 + (bv & mask) as u128 + 1) >> 1) as u64,
                VLaneOp::Sign if bv & mask == 0 => 0,
                VLaneOp::Sign if sx(bv) < 0 => 0u64.wrapping_sub(av),
                VLaneOp::Sign => av,
                VLaneOp::ShiftSigned8 | VLaneOp::ShiftSigned8Rnd => {
                    panic!("register shifts use the raw-hardware oracle in aarch64_smir_programs")
                }
                VLaneOp::AbsDiff if signed => (sx(av) - sx(bv)).unsigned_abs() as u64,
                VLaneOp::AbsDiff => {
                    let difference = i128::from(av & mask) - i128::from(bv & mask);
                    difference.unsigned_abs() as u64
                }
            } & mask;
            out[off..off + elem_bytes].copy_from_slice(&result.to_le_bytes()[..elem_bytes]);
        }

        let mut low = [0u8; 8];
        let mut high = [0u8; 8];
        low.copy_from_slice(&out[..8]);
        high.copy_from_slice(&out[8..]);
        (u64::from_le_bytes(low), u64::from_le_bytes(high))
    }

    let a_low = 0x807f_00ff_7f80_ff00;
    let a_high = 0x7fff_8000_ffff_0001;
    let b_low = 0x7f80_ff00_0080_00ff;
    let b_high = 0x8000_7fff_0001_ffff;
    let code = lower_ops(vec![
        OpKind::VLane {
            dst: v(0),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I8,
            lanes: 16,
            op: VLaneOp::Max,
            signed: true,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(3),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I16,
            lanes: 8,
            op: VLaneOp::Min,
            signed: false,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(4),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I8,
            lanes: 16,
            op: VLaneOp::AddSat,
            signed: true,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(5),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I16,
            lanes: 8,
            op: VLaneOp::SubSat,
            signed: false,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(6),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I16,
            lanes: 8,
            op: VLaneOp::Avg,
            signed: true,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(7),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I32,
            lanes: 2,
            op: VLaneOp::AvgRnd,
            signed: false,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(8),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I32,
            lanes: 4,
            op: VLaneOp::AbsDiff,
            signed: true,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(9),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I64,
            lanes: 1,
            op: VLaneOp::AndNot,
            signed: false,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(10),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I32,
            lanes: 4,
            op: VLaneOp::OrNot,
            signed: false,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(11),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I16,
            lanes: 4,
            op: VLaneOp::Mul,
            signed: true,
            set_ovf: false,
        },
        OpKind::VLane {
            dst: v(12),
            src1: v(1),
            src2: v(2),
            elem: VecElementType::I64,
            lanes: 2,
            op: VLaneOp::Not,
            signed: false,
            set_ovf: false,
        },
    ]);

    let (_, simd, _) =
        run_aarch64_code_with_regs_and_simd(&code, &[], &[(1, a_low, a_high), (2, b_low, b_high)]);
    assert_eq!(
        simd[0],
        apply_vlane(a_low, a_high, b_low, b_high, 1, 16, VLaneOp::Max, true)
    );
    assert_eq!(
        simd[3],
        apply_vlane(a_low, a_high, b_low, b_high, 2, 8, VLaneOp::Min, false)
    );
    assert_eq!(
        simd[4],
        apply_vlane(a_low, a_high, b_low, b_high, 1, 16, VLaneOp::AddSat, true)
    );
    assert_eq!(
        simd[5],
        apply_vlane(a_low, a_high, b_low, b_high, 2, 8, VLaneOp::SubSat, false)
    );
    assert_eq!(
        simd[6],
        apply_vlane(a_low, a_high, b_low, b_high, 2, 8, VLaneOp::Avg, true)
    );
    assert_eq!(
        simd[7],
        apply_vlane(a_low, a_high, b_low, b_high, 4, 2, VLaneOp::AvgRnd, false)
    );
    assert_eq!(
        simd[8],
        apply_vlane(a_low, a_high, b_low, b_high, 4, 4, VLaneOp::AbsDiff, true)
    );
    assert_eq!(
        simd[9],
        apply_vlane(a_low, a_high, b_low, b_high, 8, 1, VLaneOp::AndNot, false)
    );
    assert_eq!(
        simd[10],
        apply_vlane(a_low, a_high, b_low, b_high, 4, 4, VLaneOp::OrNot, false)
    );
    assert_eq!(
        simd[11],
        apply_vlane(a_low, a_high, b_low, b_high, 2, 4, VLaneOp::Mul, true)
    );
    assert_eq!(
        simd[12],
        apply_vlane(a_low, a_high, b_low, b_high, 8, 2, VLaneOp::Not, false)
    );
}
