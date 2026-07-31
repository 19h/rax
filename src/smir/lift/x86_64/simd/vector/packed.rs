//! packed.rs

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
    /// Merge a computed legacy packed-XMM result into the architectural
    /// destination without changing the shared YMM/ZMM state above bit 127.
    pub(crate) fn append_legacy_packed_result(
        &self,
        dst: VReg,
        result: VReg,
        elem: VecElementType,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = VecWidth::V128.lanes(elem) as u8;
        let mut scalars = Vec::with_capacity(lanes as usize);
        for lane in 0..lanes {
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: result,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            scalars.push((lane, scalar));
        }
        for (lane, scalar) in scalars {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst,
                    vec: dst,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
    }

    pub(crate) fn append_zero_vector(
        &self,
        width: VecWidth,
        elem: VecElementType,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let zero = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: zero,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        let vector = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBroadcast {
                dst: vector,
                scalar: zero,
                elem,
                lanes: width.lanes(elem) as u8,
            },
        ));
        vector
    }

    pub(crate) fn append_vector_splat_imm(
        &self,
        value: u64,
        width: VecWidth,
        elem: VecElementType,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let scalar = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: scalar,
                src: SrcOperand::Imm(value as i64),
                width: OpWidth::W64,
            },
        ));
        let vector = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBroadcast {
                dst: vector,
                scalar,
                elem,
                lanes: width.lanes(elem) as u8,
            },
        ));
        vector
    }

    pub(crate) fn append_vector_and(
        &self,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let dst = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VAnd {
                dst,
                src1,
                src2,
                width,
            },
        ));
        dst
    }

    /// Append the vector operation `!src1 & src2`.
    pub(crate) fn append_vector_and_not(
        &self,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let dst = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VAndNot {
                dst,
                src1,
                src2,
                width,
            },
        ));
        dst
    }

    pub(crate) fn append_vector_or(
        &self,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let dst = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VOr {
                dst,
                src1,
                src2,
                width,
            },
        ));
        dst
    }

    pub(crate) fn append_vector_xor(
        &self,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let dst = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VXor {
                dst,
                src1,
                src2,
                width,
            },
        ));
        dst
    }

    pub(crate) fn append_vector_sub(
        &self,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        lanes: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let dst = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VSub {
                dst,
                src1,
                src2,
                elem,
                lanes,
            },
        ));
        dst
    }

    pub(crate) fn append_vector_shift(
        &self,
        src: VReg,
        amount: u8,
        shift: ShiftOp,
        elem: VecElementType,
        lanes: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let dst = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VShift {
                dst,
                src,
                amount: SrcOperand::Imm(i64::from(amount)),
                shift,
                elem,
                lanes,
            },
        ));
        dst
    }

    /// Append the exact per-lane semantics of PSIGNB/W/D using generic SMIR:
    /// negative control -> wrapping negation, zero control -> zero, positive
    /// control -> original value.
    pub(crate) fn append_packed_sign(
        &self,
        dst: VReg,
        value: VReg,
        control: VReg,
        elem: VecElementType,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(elem) as u8;
        let zero = self.append_zero_vector(width, elem, pc, ctx, ops);
        let negated = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VUnary {
                dst: negated,
                src: value,
                elem,
                lanes,
                op: VecUnaryOp::Neg,
            },
        ));
        let negative = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst: negative,
                src1: control,
                src2: zero,
                cond: VecCmpCond::Lt,
                elem,
                lanes,
            },
        ));
        let is_zero = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst: is_zero,
                src1: control,
                src2: zero,
                cond: VecCmpCond::Eq,
                elem,
                lanes,
            },
        ));
        let signed = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBitSelect {
                dst: signed,
                mask: negative,
                src_true: negated,
                src_false: value,
                width,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VAndNot {
                dst,
                src1: is_zero,
                src2: signed,
                width,
            },
        ));
    }

    pub(crate) fn packed_minmax_shape(opcode: u8, qword: bool) -> (VecElementType, bool, bool) {
        let elem = match opcode {
            0xDA | 0xDE => VecElementType::I8,
            0xEA | 0xEE => VecElementType::I16,
            0x38 | 0x3C => VecElementType::I8,
            0x3A | 0x3E => VecElementType::I16,
            0x39 | 0x3B | 0x3D | 0x3F if qword => VecElementType::I64,
            0x39 | 0x3B | 0x3D | 0x3F => VecElementType::I32,
            _ => unreachable!(),
        };
        let min = matches!(opcode, 0x38..=0x3B | 0xDA | 0xEA);
        let signed = matches!(opcode, 0x38 | 0x39 | 0x3C | 0x3D | 0xEA | 0xEE);
        (elem, min, signed)
    }

    /// Select the elementwise signed/unsigned packed integer minimum or maximum.
    /// `VCmp` produces an all-ones lane mask, which makes the subsequent
    /// bit-select exact for every integer element width, including EVEX qwords.
    pub(crate) fn append_packed_minmax(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        width: VecWidth,
        min: bool,
        signed: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let cond = match (min, signed) {
            (true, true) => VecCmpCond::Lt,
            (true, false) => VecCmpCond::Ltu,
            (false, true) => VecCmpCond::Gt,
            (false, false) => VecCmpCond::Gtu,
        };
        let select_src1 = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst: select_src1,
                src1,
                src2,
                cond,
                elem,
                lanes: width.lanes(elem) as u8,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBitSelect {
                dst,
                mask: select_src1,
                src_true: src1,
                src_false: src2,
                width,
            },
        ));
    }

    /// Find the unsigned minimum of eight packed words and return the minimum
    /// in bits 15:0 and its first (lowest) lane index in bits 18:16.  Scalar
    /// comparisons are used so the strict-less-than tie rule is explicit.  The
    /// temporary flag changes are bracketed by ReadFlags/WriteFlags because
    /// PHMINPOSUW does not architecturally modify flags.
    pub(crate) fn append_phminposuw(
        &self,
        dst: VReg,
        src: VReg,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let saved_flags = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::ReadFlags { dst: saved_flags },
        ));

        let mut minimum = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VExtractLane {
                dst: minimum,
                vec: src,
                lane: 0,
                elem: VecElementType::I16,
                sign: SignExtend::Zero,
            },
        ));
        let mut index = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: index,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));

        for lane in 1..8u8 {
            let candidate = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: candidate,
                    vec: src,
                    lane,
                    elem: VecElementType::I16,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Cmp {
                    src1: candidate,
                    src2: SrcOperand::Reg(minimum),
                    width: OpWidth::W16,
                },
            ));
            let replace = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::SetCC {
                    dst: replace,
                    cond: Condition::Ult,
                    width: OpWidth::W64,
                },
            ));
            let next_minimum = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Select {
                    dst: next_minimum,
                    cond: replace,
                    src_true: candidate,
                    src_false: minimum,
                    width: OpWidth::W64,
                },
            ));
            let lane_index = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: lane_index,
                    src: SrcOperand::Imm(i64::from(lane)),
                    width: OpWidth::W64,
                },
            ));
            let next_index = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Select {
                    dst: next_index,
                    cond: replace,
                    src_true: lane_index,
                    src_false: index,
                    width: OpWidth::W64,
                },
            ));
            minimum = next_minimum;
            index = next_index;
        }

        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::WriteFlags { src: saved_flags },
        ));
        let shifted_index = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Shl {
                dst: shifted_index,
                src: index,
                amount: SrcOperand::Imm(16),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        let packed = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Or {
                dst: packed,
                src1: minimum,
                src2: SrcOperand::Reg(shifted_index),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        let zero = self.append_zero_vector(VecWidth::V128, VecElementType::I64, pc, ctx, ops);
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VInsertLane {
                dst,
                vec: zero,
                scalar: packed,
                lane: 0,
                elem: VecElementType::I64,
            },
        ));
    }

    pub(crate) fn pmul_high_word_kind(
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        signed: bool,
    ) -> OpKind {
        OpKind::VMulShiftSat {
            dst,
            src1,
            src2,
            src_elem: VecElementType::I16,
            lanes: width.lanes(VecElementType::I16) as u8,
            signed1: signed,
            signed2: signed,
            shift_left: 0,
            round: false,
            sat_bits: 0,
            out_shift: 16,
        }
    }

    pub(crate) fn packed_extend_shape(opcode: u8) -> (VecElementType, VecElementType, bool) {
        let signed = opcode < 0x30;
        let (src, dst) = match opcode & 0x0F {
            0x00 => (VecElementType::I8, VecElementType::I16),
            0x01 => (VecElementType::I8, VecElementType::I32),
            0x02 => (VecElementType::I8, VecElementType::I64),
            0x03 => (VecElementType::I16, VecElementType::I32),
            0x04 => (VecElementType::I16, VecElementType::I64),
            0x05 => (VecElementType::I32, VecElementType::I64),
            _ => unreachable!(),
        };
        (src, dst, signed)
    }

    pub(crate) fn packed_extend_source_width(source_bytes: u32) -> VecWidth {
        match source_bytes {
            0..=8 => VecWidth::V64,
            9..=16 => VecWidth::V128,
            17..=32 => VecWidth::V256,
            _ => unreachable!(),
        }
    }

    pub(crate) fn append_packed_extend(
        &self,
        dst: VReg,
        src: VReg,
        src_elem: VecElementType,
        dst_elem: VecElementType,
        lanes: u8,
        signed: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        // Extract every source lane before initializing `dst`, preserving exact
        // behavior when the architectural source and destination alias.
        let mut scalars = Vec::with_capacity(lanes as usize);
        for lane in 0..lanes {
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: src,
                    lane,
                    elem: src_elem,
                    sign: if signed {
                        SignExtend::Sign
                    } else {
                        SignExtend::Zero
                    },
                },
            ));
            scalars.push((lane, scalar));
        }
        let zero = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: zero,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBroadcast {
                dst,
                scalar: zero,
                elem: dst_elem,
                lanes,
            },
        ));
        for (lane, scalar) in scalars {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst,
                    vec: dst,
                    scalar,
                    lane,
                    elem: dst_elem,
                },
            ));
        }
    }

    pub(crate) fn append_packed_extend_memory_source(
        &self,
        addr: Address,
        src_elem: VecElementType,
        lanes: u8,
        mask: Option<VReg>,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let source_bytes = u32::from(lanes) * src_elem.bytes();
        let source_width = Self::packed_extend_source_width(source_bytes);
        let source = self.append_zero_vector(source_width, src_elem, pc, ctx, ops);
        let base = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Lea { dst: base, addr },
        ));
        let mem_width = match src_elem {
            VecElementType::I8 => MemWidth::B1,
            VecElementType::I16 => MemWidth::B2,
            VecElementType::I32 => MemWidth::B4,
            _ => unreachable!(),
        };
        for lane in 0..lanes {
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(0),
                    width: OpWidth::W64,
                },
            ));
            let lane_addr = Address::base_off(base, i64::from(lane) * i64::from(src_elem.bytes()));
            if let Some(mask) = mask {
                let shifted = ctx.alloc_vreg();
                let active = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shr {
                        dst: shifted,
                        src: mask,
                        amount: SrcOperand::Imm(i64::from(lane)),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::And {
                        dst: active,
                        src1: shifted,
                        src2: SrcOperand::Imm(1),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::PredLoad {
                        dst: scalar,
                        cond: active,
                        addr: lane_addr,
                        width: mem_width,
                        signed: SignExtend::Zero,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr: lane_addr,
                        width: mem_width,
                        sign: SignExtend::Zero,
                    },
                ));
            }
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: source,
                    vec: source,
                    scalar,
                    lane,
                    elem: src_elem,
                },
            ));
        }
        source
    }

    /// Construct an XMM result whose low 32- or 64-bit lane is `scalar` and
    /// whose remaining bits through bit 127 are zero. Legacy encodings retain
    /// the shared architectural backing state above bit 127; VEX/EVEX
    /// encodings clear it.
    pub(crate) fn append_scalar_zeroed_xmm_result(
        &self,
        dst: VReg,
        scalar: VReg,
        elem: VecElementType,
        zero_upper: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let zero = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Mov {
                dst: zero,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));

        let result = if zero_upper { dst } else { ctx.alloc_vreg() };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBroadcast {
                dst: result,
                scalar: zero,
                elem,
                lanes: 1,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VInsertLane {
                dst: result,
                vec: result,
                scalar,
                lane: 0,
                elem,
            },
        ));

        if !zero_upper {
            self.append_legacy_packed_result(dst, result, elem, pc, ctx, ops);
        }
    }

    pub(crate) fn append_integer_interleave(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        width: VecWidth,
        high: bool,
        hint: X86OpHint,
        pc: u64,
        ops: &mut Vec<SmirOp>,
    ) {
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VInterleave {
                dst,
                src1,
                src2,
                elem,
                lanes: width.lanes(elem) as u8,
                block_lanes: (if width == VecWidth::V64 { 8 } else { 16 } / elem.bytes()) as u8,
                high,
            },
            hint,
        ));
    }

    pub(crate) fn lift_vec_packed_extend(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.vvvv != 0
            || (prefix.encoding == VecEncodingKind::Evex && prefix.v_high)
            || prefix.l_bits == 3
            || prefix.b
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
            || (prefix.encoding == VecEncodingKind::Evex
                && matches!(opcode, 0x25 | 0x35)
                && prefix.w)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let (src_elem, dst_elem, signed) = Self::packed_extend_shape(opcode);
        let lanes = prefix.width.lanes(dst_elem) as u8;
        let source_bytes = u32::from(lanes) * src_elem.bytes();
        let source_width = Self::packed_extend_source_width(source_bytes);
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = if prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0 {
            Some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))))
        } else {
            None
        };
        let src = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    source_bytes,
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            self.append_packed_extend_memory_source(addr, src_elem, lanes, mask, pc, ctx, &mut ops)
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                source_width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if prefix.encoding == VecEncodingKind::Evex {
            let raw = ctx.alloc_vreg();
            self.append_packed_extend(
                raw, src, src_elem, dst_elem, lanes, signed, pc, ctx, &mut ops,
            );
            self.append_evex_vector_mask_result(prefix, dst, raw, dst_elem, pc, ctx, &mut ops);
        } else {
            self.append_packed_extend(
                dst, src, src_elem, dst_elem, lanes, signed, pc, ctx, &mut ops,
            );
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_packed_minmax(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let qword = prefix.encoding == VecEncodingKind::Evex
            && prefix.w
            && matches!(opcode, 0x39 | 0x3B | 0x3D | 0x3F);
        let (elem, min, signed) = Self::packed_minmax_shape(opcode, qword);
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let broadcast = prefix.encoding == VecEncodingKind::Evex
            && prefix.b
            && modrm.is_memory
            && matches!(elem, VecElementType::I32 | VecElementType::I64);
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    if broadcast {
                        elem.bytes()
                    } else {
                        prefix.width.bytes()
                    },
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    prefix.width,
                    broadcast,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if broadcast {
                let scalar = ctx.alloc_vreg();
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: if elem == VecElementType::I32 {
                            MemWidth::B4
                        } else {
                            MemWidth::B8
                        },
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: loaded,
                        scalar,
                        elem,
                        lanes: prefix.width.lanes(elem) as u8,
                    },
                ));
                loaded
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if !modrm.is_memory && (prefix.encoding == VecEncodingKind::Vex || prefix.aaa == 0) {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLane {
                    dst,
                    src1,
                    src2,
                    elem,
                    lanes: prefix.width.lanes(elem) as u8,
                    op: if min { VLaneOp::Min } else { VLaneOp::Max },
                    signed,
                    set_ovf: false,
                },
                self.vec_hint(prefix, opcode),
            ));
        } else if prefix.encoding == VecEncodingKind::Evex {
            let raw = ctx.alloc_vreg();
            self.append_packed_minmax(
                raw,
                src1,
                src2,
                elem,
                prefix.width,
                min,
                signed,
                pc,
                ctx,
                &mut ops,
            );
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        } else {
            self.append_packed_minmax(
                dst,
                src1,
                src2,
                elem,
                prefix.width,
                min,
                signed,
                pc,
                ctx,
                &mut ops,
            );
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_mpsadbw(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let vex = prefix.encoding == VecEncodingKind::Vex
            && prefix.pp == X86SsePrefix::OpSize
            && matches!(prefix.width, VecWidth::V128 | VecWidth::V256);
        let evex_mpsadbw = prefix.encoding == VecEncodingKind::Evex
            && prefix.pp == X86SsePrefix::Rep
            && !prefix.w
            && prefix.l_bits != 3
            && matches!(
                prefix.width,
                VecWidth::V128 | VecWidth::V256 | VecWidth::V512
            )
            && !prefix.b
            && (!prefix.zeroing || prefix.aaa != 0);
        let vdbpsadbw = prefix.encoding == VecEncodingKind::Evex
            && prefix.pp == X86SsePrefix::OpSize
            && !prefix.w
            && prefix.l_bits != 3
            && matches!(
                prefix.width,
                VecWidth::V128 | VecWidth::V256 | VecWidth::V512
            )
            && !prefix.b
            && (!prefix.zeroing || prefix.aaa != 0);
        if !vex && !evex_mpsadbw && !vdbpsadbw {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let evex = evex_mpsadbw || vdbpsadbw;
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    prefix.width.bytes(),
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: prefix.width,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.vec_reg(
                modrm.rm + if evex && prefix.rm_high { 16 } else { 0 },
                prefix.width,
            )
        };
        let dst = self.vec_reg(
            modrm.reg + if evex && prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let src1 = self.vec_reg(
            prefix.vvvv + if evex && prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        if vdbpsadbw {
            let raw = self.append_vdbpsadbw(
                src1,
                src2,
                prefix.width,
                bytes[imm_offset],
                pc,
                ctx,
                &mut ops,
            );
            self.append_evex_vector_mask_result(
                prefix,
                dst,
                raw,
                VecElementType::I16,
                pc,
                ctx,
                &mut ops,
            );
            return Ok(LiftResult::fallthrough(ops, imm_offset + 1));
        }

        let kind = OpKind::VMpsadbw {
            dst,
            src1,
            src2,
            mask: (evex && prefix.aaa != 0)
                .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)))),
            width: prefix.width,
            imm: bytes[imm_offset],
            zeroing: evex && prefix.zeroing,
        };
        if modrm.is_memory {
            // The register-only native admission deliberately leaves memory
            // loads in the established fault-atomic fallback path.
            ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
        } else {
            let hint = if evex {
                X86OpHint::EvexOp {
                    map: X86VecMap::Map0F3A,
                    pp: X86SsePrefix::Rep,
                    opcode: 0x42,
                    width: prefix.width,
                    w: false,
                }
            } else {
                X86OpHint::VexOp {
                    map: X86VecMap::Map0F3A,
                    pp: X86SsePrefix::OpSize,
                    opcode: 0x42,
                    width: prefix.width,
                    w: prefix.w,
                }
            };
            ops.push(SmirOp::with_hint(OpId(ops.len() as u16), pc, kind, hint));
        }
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_vec_psadbw(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.aaa != 0
            || prefix.zeroing
            || prefix.b
            || (prefix.encoding == VecEncodingKind::Vex
                && !matches!(prefix.width, VecWidth::V128 | VecWidth::V256))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: prefix.width,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let kind = OpKind::VSadBytes {
            dst,
            src1,
            src2,
            width: prefix.width,
        };
        if modrm.is_memory {
            ops.push(SmirOp::new(OpId(ops.len() as u16), pc, kind));
        } else {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                kind,
                self.vec_hint(prefix, 0xF6),
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn packed_shift_count_spec(
        opcode: u8,
        evex: bool,
        w: bool,
    ) -> Option<(VecElementType, ShiftOp, &'static str)> {
        match opcode {
            0xD1 => Some((VecElementType::I16, ShiftOp::Lsr, "PSRLW")),
            0xD2 if !evex || !w => Some((VecElementType::I32, ShiftOp::Lsr, "PSRLD")),
            0xD3 if !evex || w => Some((VecElementType::I64, ShiftOp::Lsr, "PSRLQ")),
            0xE1 => Some((VecElementType::I16, ShiftOp::Asr, "PSRAW")),
            0xE2 if evex && w => Some((VecElementType::I64, ShiftOp::Asr, "PSRAQ")),
            0xE2 => Some((VecElementType::I32, ShiftOp::Asr, "PSRAD")),
            0xF1 => Some((VecElementType::I16, ShiftOp::Lsl, "PSLLW")),
            0xF2 if !evex || !w => Some((VecElementType::I32, ShiftOp::Lsl, "PSLLD")),
            0xF3 if !evex || w => Some((VecElementType::I64, ShiftOp::Lsl, "PSLLQ")),
            _ => None,
        }
    }

    pub(crate) fn lift_vec_packed_shift_count(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let evex = prefix.encoding == VecEncodingKind::Evex;
        let Some((elem, shift, _)) = Self::packed_shift_count_spec(opcode, evex, prefix.w) else {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        };
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.b
            || (evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let count_vec = if modrm.is_memory {
            let (addr, pre_ops) = if evex {
                self.vec_disp8_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, 16, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Unaligned),
            ));
            loaded
        } else {
            self.xmm(modrm.rm + if evex && prefix.rm_high { 16 } else { 0 })
        };
        let count = if modrm.is_memory {
            let count = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: count,
                    vec: count_vec,
                    lane: 0,
                    elem: VecElementType::I64,
                    sign: SignExtend::Zero,
                },
            ));
            count
        } else {
            count_vec
        };
        let src = self.vec_reg(
            prefix.vvvv + if evex && prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg + if evex && prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let raw = if evex && prefix.aaa != 0 {
            ctx.alloc_vreg()
        } else {
            dst
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86PackedShift {
                dst: raw,
                src,
                count,
                width: prefix.width,
                elem,
                shift,
            },
        ));
        if evex && prefix.aaa != 0 {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_packed_shift_variable(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let evex = prefix.encoding == VecEncodingKind::Evex;
        if prefix.map != X86VecMap::Map0F38
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || (evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let (elem, shift) = match (opcode, prefix.w, evex) {
            (0x10, true, true) => (VecElementType::I16, ShiftOp::Lsr),
            (0x11, true, true) => (VecElementType::I16, ShiftOp::Asr),
            (0x12, true, true) => (VecElementType::I16, ShiftOp::Lsl),
            (0x45, false, _) => (VecElementType::I32, ShiftOp::Lsr),
            (0x45, true, _) => (VecElementType::I64, ShiftOp::Lsr),
            (0x46, false, _) => (VecElementType::I32, ShiftOp::Asr),
            (0x46, true, true) => (VecElementType::I64, ShiftOp::Asr),
            (0x47, false, _) => (VecElementType::I32, ShiftOp::Lsl),
            (0x47, true, _) => (VecElementType::I64, ShiftOp::Lsl),
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            address_size_override: prefix.address_size_override,
            segment_override: prefix.segment_override,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let broadcast = evex
            && prefix.b
            && modrm.is_memory
            && matches!(elem, VecElementType::I32 | VecElementType::I64);
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let count = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                if broadcast {
                    elem.bytes()
                } else {
                    prefix.width.bytes()
                },
                ctx,
            );
            ops.extend(pre_ops);
            if evex && prefix.aaa != 0 {
                self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    prefix.width,
                    broadcast,
                    VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))),
                    pc,
                    ctx,
                    &mut ops,
                )
            } else if broadcast {
                self.append_broadcast_memory_source(addr, elem, prefix.width, pc, ctx, &mut ops)
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm + if evex && prefix.rm_high { 16 } else { 0 },
                prefix.width,
            )
        };
        let src = self.vec_reg(
            prefix.vvvv + if evex && prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg + if evex && prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86PackedShiftVariable {
                dst,
                src,
                count,
                mask: (evex && prefix.aaa != 0)
                    .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)))),
                width: prefix.width,
                elem,
                shift,
                zeroing: evex && prefix.zeroing,
            },
        ));
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_packed_shift_imm(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let evex = prefix.encoding == VecEncodingKind::Evex;
        if prefix.pp != X86SsePrefix::OpSize
            || (prefix.encoding == VecEncodingKind::Vex
                && !matches!(prefix.width, VecWidth::V128 | VecWidth::V256))
            || (evex && prefix.l_bits == 3)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = prefix.modrm_prefix(cursor);
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let group = (modrm.byte >> 3) & 7;
        let (elem, shift, byte_lane, rotate_left) = match (opcode, group, evex && prefix.w) {
            (0x71, 2, _) => (VecElementType::I16, ShiftOp::Lsr, false, None),
            (0x71, 4, _) => (VecElementType::I16, ShiftOp::Asr, false, None),
            (0x71, 6, _) => (VecElementType::I16, ShiftOp::Lsl, false, None),
            (0x72, 0, w) if evex => (
                if w {
                    VecElementType::I64
                } else {
                    VecElementType::I32
                },
                ShiftOp::Lsr,
                false,
                Some(false),
            ),
            (0x72, 1, w) if evex => (
                if w {
                    VecElementType::I64
                } else {
                    VecElementType::I32
                },
                ShiftOp::Lsl,
                false,
                Some(true),
            ),
            (0x72, 2, false) => (VecElementType::I32, ShiftOp::Lsr, false, None),
            (0x72, 4, false) => (VecElementType::I32, ShiftOp::Asr, false, None),
            (0x72, 4, true) if evex => (VecElementType::I64, ShiftOp::Asr, false, None),
            (0x72, 6, false) => (VecElementType::I32, ShiftOp::Lsl, false, None),
            (0x73, 2, true) if evex => (VecElementType::I64, ShiftOp::Lsr, false, None),
            (0x73, 2, false) if !evex => (VecElementType::I64, ShiftOp::Lsr, false, None),
            (0x73, 3, _) => (VecElementType::I8, ShiftOp::Lsr, true, None),
            (0x73, 6, true) if evex => (VecElementType::I64, ShiftOp::Lsl, false, None),
            (0x73, 6, false) if !evex => (VecElementType::I64, ShiftOp::Lsl, false, None),
            (0x73, 7, _) => (VecElementType::I8, ShiftOp::Lsl, true, None),
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        if evex
            && ((byte_lane && (prefix.aaa != 0 || prefix.zeroing || prefix.b))
                || (!byte_lane && prefix.zeroing && prefix.aaa == 0)
                || (prefix.b && !modrm.is_memory)
                || (elem == VecElementType::I16 && prefix.b))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let lanes = prefix.width.lanes(elem) as u8;
        let mask = if evex && !byte_lane && prefix.aaa != 0 {
            Some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))))
        } else {
            None
        };
        let src = if modrm.is_memory {
            if !evex {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
            let broadcast = prefix.b;
            let e4nf = byte_lane || elem == VecElementType::I16;
            let tuple_bytes = if broadcast {
                elem.bytes()
            } else {
                prefix.width.bytes()
            };
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                tuple_bytes,
                ctx,
            );
            ops.extend(pre_ops);
            if e4nf {
                let value = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: value,
                        addr,
                        width: prefix.width,
                    },
                ));
                value
            } else if broadcast {
                if let Some(mask) = mask {
                    self.append_masked_broadcast_memory_source(
                        addr,
                        elem,
                        prefix.width,
                        mask,
                        pc,
                        ctx,
                        &mut ops,
                    )
                } else {
                    self.append_broadcast_memory_source(addr, elem, prefix.width, pc, ctx, &mut ops)
                }
            } else if let Some(mask_reg) = mask {
                let value = ctx.alloc_vreg();
                let zero = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: zero,
                        src: SrcOperand::Imm(0),
                        width: OpWidth::W64,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: value,
                        scalar: zero,
                        elem,
                        lanes,
                    },
                ));
                let base = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Lea { dst: base, addr },
                ));
                for lane in 0..lanes {
                    let shifted = ctx.alloc_vreg();
                    let active = ctx.alloc_vreg();
                    let scalar = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Shr {
                            dst: shifted,
                            src: mask_reg,
                            amount: SrcOperand::Imm(i64::from(lane)),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::And {
                            dst: active,
                            src1: shifted,
                            src2: SrcOperand::Imm(1),
                            width: OpWidth::W64,
                            flags: FlagUpdate::None,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::Mov {
                            dst: scalar,
                            src: SrcOperand::Imm(0),
                            width: OpWidth::W64,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::PredLoad {
                            dst: scalar,
                            cond: active,
                            addr: Address::base_off(
                                base,
                                i64::from(lane) * i64::from(elem.bytes()),
                            ),
                            width: if elem == VecElementType::I32 {
                                MemWidth::B4
                            } else {
                                MemWidth::B8
                            },
                            signed: SignExtend::Zero,
                        },
                    ));
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VInsertLane {
                            dst: value,
                            vec: value,
                            scalar,
                            lane,
                            elem,
                        },
                    ));
                }
                value
            } else {
                let value = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: value,
                        addr,
                        width: prefix.width,
                    },
                ));
                value
            }
        } else {
            self.vec_reg(
                modrm.rm + if evex && prefix.rm_high { 16 } else { 0 },
                prefix.width,
            )
        };
        let dst = self.vec_reg(
            prefix.vvvv + if evex && prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        let raw = if rotate_left.is_some() {
            dst
        } else if evex && !byte_lane && mask.is_some() {
            ctx.alloc_vreg()
        } else {
            dst
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            if let Some(left) = rotate_left {
                OpKind::X86PackedRotate {
                    dst: raw,
                    src,
                    count: None,
                    mask,
                    amount: bytes[imm_offset],
                    width: prefix.width,
                    elem,
                    left,
                    zeroing: prefix.zeroing,
                }
            } else {
                OpKind::X86PackedShiftImm {
                    dst: raw,
                    src,
                    width: prefix.width,
                    elem,
                    shift,
                    amount: bytes[imm_offset],
                    byte_lane,
                }
            },
        ));
        if rotate_left.is_none() && evex && !byte_lane && mask.is_some() {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_vec_packed_average(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || prefix.b
            || (prefix.encoding == VecEncodingKind::Evex && prefix.zeroing && prefix.aaa == 0)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = if opcode == 0xE0 {
            VecElementType::I8
        } else {
            VecElementType::I16
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let src2 = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    prefix.width,
                    false,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                )
            } else {
                let loaded = ctx.alloc_vreg();
                ops.push(SmirOp::with_hint(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VLoad {
                        dst: loaded,
                        addr,
                        width: prefix.width,
                    },
                    X86OpHint::VecAlign(X86VecAlign::Unaligned),
                ));
                loaded
            }
        } else {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let src1 = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let dst = self.vec_reg(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        if !modrm.is_memory && (prefix.encoding == VecEncodingKind::Vex || prefix.aaa == 0) {
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                Self::packed_unsigned_average_kind(dst, src1, src2, prefix.width, elem),
                self.vec_hint(prefix, opcode),
            ));
        } else if prefix.encoding == VecEncodingKind::Evex {
            let raw = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                Self::packed_unsigned_average_kind(raw, src1, src2, prefix.width, elem),
            ));
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                Self::packed_unsigned_average_kind(dst, src1, src2, prefix.width, elem),
            ));
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }
}
