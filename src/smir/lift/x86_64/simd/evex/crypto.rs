//! crypto.rs

use crate::smir::lift::x86_64::*;
use std::collections::{HashMap, HashSet};

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86ThreeDNowKind, X86VecAlign, X86VecMap,
    X86X87ArithmeticDestination, X86X87ArithmeticSource, X86X87CompareSource, X86X87Constant,
    X86X87ControlKind, X86X87DataKind, X86X87EnvWidth, X86X87FloatWidth, X86X87IntWidth,
    X86XSaveKind,
};
use crate::smir::ir::types::*;
use crate::smir::ir::{
    CallTarget, CallingConv, FunctionAttrs, SmirBlock, SmirFunction, Terminator, TrapKind,
    X86InstructionBytes,
};

impl X86_64Lifter {


    pub(crate) fn append_gf2p8_mul_vector(
        &self,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let elem = VecElementType::I8;
        let lanes = width.lanes(elem) as u8;
        let zero = self.append_zero_vector(width, elem, pc, ctx, ops);
        let one = self.append_vector_splat_imm(1, width, elem, pc, ctx, ops);
        let reduction_polynomial = self.append_vector_splat_imm(0x1B, width, elem, pc, ctx, ops);
        let mut result = zero;
        let mut multiplicand = src1;
        let mut multiplier = src2;

        // Russian-peasant multiplication in GF(2^8). Each byte lane is
        // independent and reduction uses x^8 + x^4 + x^3 + x + 1 (0x11B).
        for _ in 0..8 {
            let multiplier_lsb = self.append_vector_and(multiplier, one, width, pc, ctx, ops);
            let multiplier_mask =
                self.append_vector_sub(zero, multiplier_lsb, elem, lanes, pc, ctx, ops);
            let contribution =
                self.append_vector_and(multiplicand, multiplier_mask, width, pc, ctx, ops);
            result = self.append_vector_xor(result, contribution, width, pc, ctx, ops);

            let carry =
                self.append_vector_shift(multiplicand, 7, ShiftOp::Lsr, elem, lanes, pc, ctx, ops);
            let carry_mask = self.append_vector_sub(zero, carry, elem, lanes, pc, ctx, ops);
            let reduction =
                self.append_vector_and(carry_mask, reduction_polynomial, width, pc, ctx, ops);
            let shifted =
                self.append_vector_shift(multiplicand, 1, ShiftOp::Lsl, elem, lanes, pc, ctx, ops);
            multiplicand = self.append_vector_xor(shifted, reduction, width, pc, ctx, ops);
            multiplier =
                self.append_vector_shift(multiplier, 1, ShiftOp::Lsr, elem, lanes, pc, ctx, ops);
        }
        result
    }



    pub(crate) fn append_gf2p8_inverse_vector(
        &self,
        src: VReg,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        // In GF(2^8), x^-1 = x^254 for x != 0; the same addition chain maps
        // zero to zero, matching the architectural inverse table.
        let mut power = self.append_gf2p8_mul_vector(src, src, width, pc, ctx, ops);
        let mut result = power;
        for _ in 0..6 {
            power = self.append_gf2p8_mul_vector(power, power, width, pc, ctx, ops);
            result = self.append_gf2p8_mul_vector(result, power, width, pc, ctx, ops);
        }
        result
    }



    pub(crate) fn append_gf2p8_affine_vector(
        &self,
        src: VReg,
        matrix: VReg,
        width: VecWidth,
        imm: u8,
        inverse: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let elem = VecElementType::I8;
        let lanes = width.lanes(elem) as u8;
        let input = if inverse {
            self.append_gf2p8_inverse_vector(src, width, pc, ctx, ops)
        } else {
            src
        };
        let zero = self.append_zero_vector(width, elem, pc, ctx, ops);
        let one = self.append_vector_splat_imm(1, width, elem, pc, ctx, ops);
        let mut result = zero;

        for output_bit in 0..8u8 {
            let control =
                self.append_vector_splat_imm(u64::from(7 - output_bit), width, elem, pc, ctx, ops);
            let matrix_row = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VByteShuffle {
                    dst: matrix_row,
                    src: matrix,
                    control,
                    lanes,
                    block_lanes: 8,
                },
            ));
            let mut parity = self.append_vector_and(matrix_row, input, width, pc, ctx, ops);
            for shift in [4, 2, 1] {
                let high = self.append_vector_shift(
                    parity,
                    shift,
                    ShiftOp::Lsr,
                    elem,
                    lanes,
                    pc,
                    ctx,
                    ops,
                );
                parity = self.append_vector_xor(parity, high, width, pc, ctx, ops);
            }
            parity = self.append_vector_and(parity, one, width, pc, ctx, ops);
            if output_bit != 0 {
                parity = self.append_vector_shift(
                    parity,
                    output_bit,
                    ShiftOp::Lsl,
                    elem,
                    lanes,
                    pc,
                    ctx,
                    ops,
                );
            }
            result = self.append_vector_or(result, parity, width, pc, ctx, ops);
        }

        let constant = self.append_vector_splat_imm(u64::from(imm), width, elem, pc, ctx, ops);
        self.append_vector_xor(result, constant, width, pc, ctx, ops)
    }
}
