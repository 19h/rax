//! shuffle.rs

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
    pub(crate) fn append_permute_immediate_indices(
        &self,
        width: VecWidth,
        elem: VecElementType,
        imm: u8,
        table_domain_lanes: u8,
        control_repeat_lanes: u8,
        control_bits: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let indices = self.append_zero_vector(width, elem, pc, ctx, ops);
        let lanes = width.lanes(elem) as u8;
        let selector_mask = (1u8 << control_bits) - 1;
        for lane in 0..lanes {
            let domain_base = lane / table_domain_lanes * table_domain_lanes;
            let shift = (lane % control_repeat_lanes) * control_bits;
            let selector = (imm >> shift) & selector_mask;
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(i64::from(domain_base + selector)),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: indices,
                    vec: indices,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
        indices
    }

    pub(crate) fn append_permil_variable_indices(
        &self,
        controls: VReg,
        width: VecWidth,
        elem: VecElementType,
        domain_lanes: u8,
        control_shift: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let indices = self.append_zero_vector(width, elem, pc, ctx, ops);
        let lanes = width.lanes(elem) as u8;
        for lane in 0..lanes {
            let control = ctx.alloc_vreg();
            let shifted = ctx.alloc_vreg();
            let selected = ctx.alloc_vreg();
            let absolute = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: control,
                    vec: controls,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            if control_shift == 0 {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Mov {
                        dst: shifted,
                        src: SrcOperand::Reg(control),
                        width: OpWidth::W64,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Shr {
                        dst: shifted,
                        src: control,
                        amount: SrcOperand::Imm(i64::from(control_shift)),
                        width: OpWidth::W64,
                        flags: FlagUpdate::None,
                    },
                ));
            }
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::And {
                    dst: selected,
                    src1: shifted,
                    src2: SrcOperand::Imm(i64::from(domain_lanes - 1)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Or {
                    dst: absolute,
                    src1: selected,
                    src2: SrcOperand::Imm(i64::from(lane / domain_lanes * domain_lanes)),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: indices,
                    vec: indices,
                    scalar: absolute,
                    lane,
                    elem,
                },
            ));
        }
        indices
    }

    pub(crate) fn append_masked_permute_memory_result(
        &self,
        addr: Address,
        indices: VReg,
        width: VecWidth,
        elem: VecElementType,
        broadcast: bool,
        mask: VReg,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) -> VReg {
        let result = self.append_zero_vector(width, elem, pc, ctx, ops);
        let lanes = width.lanes(elem) as u8;
        let base = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Lea { dst: base, addr },
        ));
        let mem_width = match elem {
            VecElementType::I8 => MemWidth::B1,
            VecElementType::I16 => MemWidth::B2,
            VecElementType::I32 | VecElementType::F32 => MemWidth::B4,
            VecElementType::I64 | VecElementType::F64 => MemWidth::B8,
            _ => unreachable!(),
        };
        for lane in 0..lanes {
            let shifted_mask = ctx.alloc_vreg();
            let active = ctx.alloc_vreg();
            let selected = ctx.alloc_vreg();
            let bounded = ctx.alloc_vreg();
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Shr {
                    dst: shifted_mask,
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
                    src1: shifted_mask,
                    src2: SrcOperand::Imm(1),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: selected,
                    vec: indices,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::And {
                    dst: bounded,
                    src1: selected,
                    src2: SrcOperand::Imm(i64::from(lanes - 1)),
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
                    addr: if broadcast {
                        Address::Direct(base)
                    } else {
                        Address::BaseIndexScale {
                            base: Some(base),
                            index: bounded,
                            scale: elem.bytes() as u8,
                            disp: 0,
                            disp_size: DispSize::Auto,
                        }
                    },
                    width: mem_width,
                    signed: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: result,
                    vec: result,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
        result
    }

    pub(crate) fn append_packed_shuffle_imm(
        &self,
        dst: VReg,
        src: VReg,
        width: VecWidth,
        elem: VecElementType,
        imm: u8,
        high_words: Option<bool>,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(elem) as u8;
        let block_lanes = if elem == VecElementType::I32 { 4 } else { 8 };
        let indices = self.append_zero_vector(width, elem, pc, ctx, ops);
        for lane in 0..lanes {
            let within = lane % block_lanes;
            let block = lane - within;
            let shuffled = match high_words {
                None => true,
                Some(true) => within >= 4,
                Some(false) => within < 4,
            };
            let selector = if shuffled {
                let output = within % 4;
                block + if high_words == Some(true) { 4 } else { 0 } + ((imm >> (output * 2)) & 3)
            } else {
                lane
            };
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(i64::from(selector)),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: indices,
                    vec: indices,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VShuffle {
                dst,
                src1: src,
                src2: None,
                indices,
                elem,
                lanes,
            },
        ));
    }

    pub(crate) fn append_two_source_shuffle_imm(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        width: VecWidth,
        elem: VecElementType,
        imm: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(elem) as u8;
        let block_lanes = if elem == VecElementType::F32 { 4 } else { 2 };
        let indices = self.append_zero_vector(width, elem, pc, ctx, ops);
        for lane in 0..lanes {
            let within = lane % block_lanes;
            let block = lane - within;
            let (from_second, control) = if elem == VecElementType::F32 {
                (within >= 2, (imm >> (within * 2)) & 3)
            } else {
                (within == 1, (imm >> lane) & 1)
            };
            let selector = block + control + if from_second { lanes } else { 0 };
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(i64::from(selector)),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: indices,
                    vec: indices,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VShuffle {
                dst,
                src1,
                src2: Some(src2),
                indices,
                elem,
                lanes,
            },
        ));
    }

    pub(crate) fn append_duplicate_shuffle(
        &self,
        dst: VReg,
        src: VReg,
        width: VecWidth,
        elem: VecElementType,
        high: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(elem) as u8;
        let indices = self.append_zero_vector(width, elem, pc, ctx, ops);
        for lane in 0..lanes {
            let selector = lane / 2 * 2 + u8::from(high);
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(i64::from(selector)),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: indices,
                    vec: indices,
                    scalar,
                    lane,
                    elem,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VShuffle {
                dst,
                src1: src,
                src2: None,
                indices,
                elem,
                lanes,
            },
        ));
    }

    /// Blend raw element bits according to the sign bit of each mask element.
    /// Floating-point blend forms deliberately use I32/I64 comparisons so NaN
    /// payloads and non-canonical bit patterns are copied without FP semantics.
    pub(crate) fn append_variable_blend(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        mask: VReg,
        elem: VecElementType,
        width: VecWidth,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let zero = self.append_zero_vector(width, elem, pc, ctx, ops);
        let select_src2 = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst: select_src2,
                src1: mask,
                src2: zero,
                cond: VecCmpCond::Lt,
                elem,
                lanes: width.lanes(elem) as u8,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBitSelect {
                dst,
                mask: select_src2,
                src_true: src2,
                src_false: src1,
                width,
            },
        ));
    }

    /// Blend raw integer lane bits under an immediate mask. Floating-point
    /// forms deliberately use I32/I64 lanes so NaN payloads are copied without
    /// invoking FP semantics. PBLENDW repeats imm8 for each 128-bit block.
    pub(crate) fn append_immediate_blend(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        width: VecWidth,
        imm: u8,
        repeat_128: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(elem) as u8;
        let block_lanes = (16 / elem.bytes()) as u8;
        let mut selected = Vec::with_capacity(lanes as usize);
        for lane in 0..lanes {
            let bit = if repeat_128 { lane % block_lanes } else { lane };
            let source = if (imm >> bit) & 1 != 0 { src2 } else { src1 };
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VExtractLane {
                    dst: scalar,
                    vec: source,
                    lane,
                    elem,
                    sign: SignExtend::Zero,
                },
            ));
            selected.push(scalar);
        }
        let output = self.append_zero_vector(width, elem, pc, ctx, ops);
        for (lane, scalar) in selected.into_iter().enumerate() {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: output,
                    vec: output,
                    scalar,
                    lane: lane as u8,
                    elem,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: output,
                width,
            },
        ));
    }

    pub(crate) fn append_insert_scalar_lane(
        &self,
        dst: VReg,
        merge: VReg,
        scalar: VReg,
        elem: VecElementType,
        lane: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = VecWidth::V128.lanes(elem) as u8;
        let mut values = Vec::with_capacity(lanes as usize);
        for current in 0..lanes {
            if current == lane {
                values.push(scalar);
            } else {
                let value = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: value,
                        vec: merge,
                        lane: current,
                        elem,
                        sign: SignExtend::Zero,
                    },
                ));
                values.push(value);
            }
        }
        let output = self.append_zero_vector(VecWidth::V128, elem, pc, ctx, ops);
        for (current, value) in values.into_iter().enumerate() {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: output,
                    vec: output,
                    scalar: value,
                    lane: current as u8,
                    elem,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: output,
                width: VecWidth::V128,
            },
        ));
    }

    pub(crate) fn append_insertps(
        &self,
        dst: VReg,
        merge: VReg,
        inserted: VReg,
        destination_lane: u8,
        zero_mask: u8,
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
        let mut values = Vec::with_capacity(4);
        for lane in 0..4u8 {
            if (zero_mask >> lane) & 1 != 0 {
                values.push(zero);
            } else if lane == destination_lane {
                values.push(inserted);
            } else {
                let value = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: value,
                        vec: merge,
                        lane,
                        elem: VecElementType::I32,
                        sign: SignExtend::Zero,
                    },
                ));
                values.push(value);
            }
        }
        let output = self.append_zero_vector(VecWidth::V128, VecElementType::I32, pc, ctx, ops);
        for (lane, value) in values.into_iter().enumerate() {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: output,
                    vec: output,
                    scalar: value,
                    lane: lane as u8,
                    elem: VecElementType::I32,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VMov {
                dst,
                src: output,
                width: VecWidth::V128,
            },
        ));
    }

    /// Build PALIGNR's `(high || low)` concatenations and extract one result
    /// from each architectural block (8 bytes for MMX, 16 bytes otherwise).
    pub(crate) fn append_align_right(
        &self,
        dst: VReg,
        high: VReg,
        low: VReg,
        width: VecWidth,
        offset: u8,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(VecElementType::I8) as u8;
        let block_lanes = if width == VecWidth::V64 { 8 } else { 16 };
        let indices = self.append_zero_vector(width, VecElementType::I8, pc, ctx, ops);
        for lane in 0..lanes {
            let block_base = lane / block_lanes * block_lanes;
            let in_block = lane % block_lanes;
            let concatenated = u16::from(offset) + u16::from(in_block);
            let block_lanes = u16::from(block_lanes);
            let selector = if concatenated < block_lanes {
                u16::from(block_base) + concatenated
            } else if concatenated < block_lanes * 2 {
                u16::from(lanes) + u16::from(block_base) + concatenated - block_lanes
            } else {
                u16::from(lanes) * 2
            };
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(i64::from(selector)),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: indices,
                    vec: indices,
                    scalar,
                    lane,
                    elem: VecElementType::I8,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VShuffle {
                dst,
                src1: low,
                src2: Some(high),
                indices,
                elem: VecElementType::I8,
                lanes,
            },
        ));
    }

    pub(crate) fn append_unpack_shuffle(
        &self,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        elem: VecElementType,
        width: VecWidth,
        high: bool,
        pc: u64,
        ctx: &mut LiftContext,
        ops: &mut Vec<SmirOp>,
    ) {
        let lanes = width.lanes(elem) as u8;
        let block_lanes = (16 / elem.bytes()) as u8;
        let half = block_lanes / 2;
        let zero = ctx.alloc_vreg();
        let indices = ctx.alloc_vreg();
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
                dst: indices,
                scalar: zero,
                elem,
                lanes,
            },
        ));
        for output in 0..lanes {
            let within_block = output % block_lanes;
            let block_base = output - within_block;
            let source_lane = block_base + if high { half } else { 0 } + within_block / 2;
            let selector = if within_block & 1 == 0 {
                source_lane
            } else {
                lanes + source_lane
            };
            let scalar = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: scalar,
                    src: SrcOperand::Imm(i64::from(selector)),
                    width: OpWidth::W64,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VInsertLane {
                    dst: indices,
                    vec: indices,
                    scalar,
                    lane: output,
                    elem,
                },
            ));
        }
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VShuffle {
                dst,
                src1,
                src2: Some(src2),
                indices,
                elem,
                lanes,
            },
        ));
    }

    pub(crate) fn lift_vec_vzero(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.encoding != VecEncodingKind::Vex
            || prefix.pp != X86SsePrefix::None
            || prefix.vvvv != 0
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let mut ops = Vec::new();
        let zero = self.append_zero_vector(VecWidth::V512, VecElementType::I64, pc, ctx, &mut ops);
        for index in 0u8..16 {
            let dst = self.xmm(index);
            if prefix.width == VecWidth::V128 {
                let low0 = ctx.alloc_vreg();
                let low1 = ctx.alloc_vreg();
                let result = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: low0,
                        vec: dst,
                        lane: 0,
                        elem: VecElementType::I64,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: low1,
                        vec: dst,
                        lane: 1,
                        elem: VecElementType::I64,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst: result,
                        vec: zero,
                        scalar: low0,
                        lane: 0,
                        elem: VecElementType::I64,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VInsertLane {
                        dst: result,
                        vec: result,
                        scalar: low1,
                        lane: 1,
                        elem: VecElementType::I64,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VMov {
                        dst,
                        src: result,
                        width: VecWidth::V512,
                    },
                ));
            } else {
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VMov {
                        dst,
                        src: zero,
                        width: VecWidth::V512,
                    },
                ));
            }
        }
        Ok(LiftResult::fallthrough(ops, prefix.bytes + 1))
    }

    pub(crate) fn lift_vec_permute_variable(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || (prefix.zeroing && prefix.aaa == 0)
            || prefix.l_bits == 3
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let permil = matches!(opcode, 0x0C | 0x0D);
        let elem = match opcode {
            0x0C => VecElementType::F32,
            0x0D => VecElementType::F64,
            0x16 if prefix.w => VecElementType::F64,
            0x16 => VecElementType::F32,
            0x36 if prefix.w => VecElementType::I64,
            0x36 => VecElementType::I32,
            0x8D if prefix.w => VecElementType::I16,
            0x8D => VecElementType::I8,
            _ => unreachable!(),
        };
        let valid = match (prefix.encoding, opcode) {
            (VecEncodingKind::Vex, 0x0C | 0x0D) => !prefix.w,
            (VecEncodingKind::Evex, 0x0C) => !prefix.w,
            (VecEncodingKind::Evex, 0x0D) => prefix.w,
            (VecEncodingKind::Vex, 0x16 | 0x36) => !prefix.w && prefix.width == VecWidth::V256,
            (VecEncodingKind::Evex, 0x16 | 0x36) => prefix.width != VecWidth::V128,
            (VecEncodingKind::Evex, 0x8D) => true,
            _ => false,
        };
        if !valid {
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
        if prefix.b && (!modrm.is_memory || opcode == 0x8D) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b;
        let mut ops = Vec::new();
        let memory_addr = if modrm.is_memory {
            let scale = if broadcast {
                elem.bytes()
            } else {
                prefix.width.bytes()
            };
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                scale,
                ctx,
            );
            ops.extend(pre_ops);
            Some(addr)
        } else {
            None
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
        let vvvv = self.vec_reg(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
            prefix.width,
        );
        let rm_reg = (!modrm.is_memory).then(|| {
            self.vec_reg(
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        });
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));

        let controls = if permil {
            if let Some(addr) = memory_addr.clone() {
                // EVEX VPERMILPS/PD are exception class E4NF: their memory
                // control operand does not support writemask fault
                // suppression. The mask applies only to the destination.
                if broadcast {
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
                rm_reg.unwrap()
            }
        } else {
            vvvv
        };
        let indices = if permil {
            self.append_permil_variable_indices(
                controls,
                prefix.width,
                elem,
                if elem == VecElementType::F32 { 4 } else { 2 },
                if elem == VecElementType::F64 { 1 } else { 0 },
                pc,
                ctx,
                &mut ops,
            )
        } else {
            controls
        };

        let table = if permil {
            Some(vvvv)
        } else if let Some(addr) = memory_addr {
            if let Some(mask) = mask {
                let raw = self.append_masked_permute_memory_result(
                    addr,
                    indices,
                    prefix.width,
                    elem,
                    broadcast,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                );
                self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
                return Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed));
            }
            if broadcast {
                Some(self.append_broadcast_memory_source(
                    addr,
                    elem,
                    prefix.width,
                    pc,
                    ctx,
                    &mut ops,
                ))
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
                Some(loaded)
            }
        } else {
            rm_reg
        }
        .unwrap();

        let direct_vbmi =
            prefix.encoding == VecEncodingKind::Evex && opcode == 0x8D && !modrm.is_memory;
        let raw = if prefix.encoding == VecEncodingKind::Evex && !direct_vbmi {
            ctx.alloc_vreg()
        } else {
            dst
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            if direct_vbmi {
                OpKind::X86PermuteBytesWords {
                    dst: raw,
                    table1: table,
                    table2: None,
                    indices,
                    mask,
                    elem,
                    width: prefix.width,
                    overwrite_table: false,
                    zeroing: prefix.zeroing,
                }
            } else {
                OpKind::VPermute {
                    dst: raw,
                    src1: table,
                    src2: None,
                    indices,
                    elem,
                    width: prefix.width,
                    overwrite_table: false,
                }
            },
        ));
        if prefix.encoding == VecEncodingKind::Evex && !direct_vbmi {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_permute_immediate(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.vvvv != 0
            || prefix.v_high
            || (prefix.zeroing && prefix.aaa == 0)
            || prefix.l_bits == 3
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match opcode {
            0x04 => VecElementType::F32,
            0x05 | 0x01 => VecElementType::F64,
            0x00 => VecElementType::I64,
            _ => unreachable!(),
        };
        let permil = matches!(opcode, 0x04 | 0x05);
        let valid = if permil {
            match prefix.encoding {
                VecEncodingKind::Vex => !prefix.w,
                VecEncodingKind::Evex => prefix.w == (elem == VecElementType::F64),
            }
        } else {
            prefix.w && prefix.width != VecWidth::V128
        };
        if !valid {
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
        if prefix.b && !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let imm_offset = cursor + modrm.bytes_consumed;
        let Some(&imm) = bytes.get(imm_offset) else {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        };
        let next_pc = pc + imm_offset as u64 + 1;
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b;
        let mut ops = Vec::new();
        let indices = self.append_permute_immediate_indices(
            prefix.width,
            elem,
            imm,
            if elem == VecElementType::F32 {
                4
            } else if permil {
                2
            } else {
                4
            },
            if elem == VecElementType::F64 && permil {
                8
            } else {
                4
            },
            if elem == VecElementType::F64 && permil {
                1
            } else {
                2
            },
            pc,
            ctx,
            &mut ops,
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
        let mask = (prefix.encoding == VecEncodingKind::Evex && prefix.aaa != 0)
            .then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
        let table = if modrm.is_memory {
            let scale = if broadcast {
                elem.bytes()
            } else {
                prefix.width.bytes()
            };
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                scale,
                ctx,
            );
            ops.extend(pre_ops);
            if let Some(mask) = mask {
                let raw = self.append_masked_permute_memory_result(
                    addr,
                    indices,
                    prefix.width,
                    elem,
                    broadcast,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                );
                self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
                return Ok(LiftResult::fallthrough(ops, imm_offset + 1));
            }
            if broadcast {
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
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
            )
        };
        let raw = if prefix.encoding == VecEncodingKind::Evex {
            ctx.alloc_vreg()
        } else {
            dst
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VPermute {
                dst: raw,
                src1: table,
                src2: None,
                indices,
                elem,
                width: prefix.width,
                overwrite_table: false,
            },
        ));
        if prefix.encoding == VecEncodingKind::Evex {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_vec_packed_shuffle_imm(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let dword = prefix.pp == X86SsePrefix::OpSize;
        if !matches!(
            prefix.encoding,
            VecEncodingKind::Vex | VecEncodingKind::Evex
        ) || !matches!(
            prefix.pp,
            X86SsePrefix::OpSize | X86SsePrefix::Rep | X86SsePrefix::Repne
        ) || prefix.vvvv != 0
            || (prefix.encoding == VecEncodingKind::Vex
                && !matches!(prefix.width, VecWidth::V128 | VecWidth::V256))
            || (prefix.encoding == VecEncodingKind::Evex
                && (prefix.v_high
                    || prefix.l_bits == 3
                    || (prefix.zeroing && prefix.aaa == 0)
                    || (dword && prefix.w)
                    || (!dword && prefix.b)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let broadcast =
            prefix.encoding == VecEncodingKind::Evex && dword && prefix.b && modrm.is_memory;
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
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
        let src = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    if broadcast { 4 } else { prefix.width.bytes() },
                    ctx,
                )
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            if broadcast {
                let scalar = ctx.alloc_vreg();
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: MemWidth::B4,
                        sign: SignExtend::Zero,
                    },
                ));
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VBroadcast {
                        dst: vector,
                        scalar,
                        elem: VecElementType::I32,
                        lanes: prefix.width.lanes(VecElementType::I32) as u8,
                    },
                ));
                vector
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
        let (elem, high_words) = match prefix.pp {
            X86SsePrefix::OpSize => (VecElementType::I32, None),
            X86SsePrefix::Rep => (VecElementType::I16, Some(true)),
            X86SsePrefix::Repne => (VecElementType::I16, Some(false)),
            X86SsePrefix::None => unreachable!(),
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
        let raw = if prefix.encoding == VecEncodingKind::Evex {
            ctx.alloc_vreg()
        } else {
            dst
        };
        self.append_packed_shuffle_imm(
            raw,
            src,
            prefix.width,
            elem,
            bytes[imm_offset],
            high_words,
            pc,
            ctx,
            &mut ops,
        );
        if prefix.encoding == VecEncodingKind::Evex {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_vec_duplicate_move(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let (elem, high) = match (opcode, prefix.pp) {
            (0x12, X86SsePrefix::Rep) => (VecElementType::F32, false),
            (0x16, X86SsePrefix::Rep) => (VecElementType::F32, true),
            (0x12, X86SsePrefix::Repne) => (VecElementType::F64, false),
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        if prefix.vvvv != 0
            || (prefix.encoding == VecEncodingKind::Vex
                && !matches!(prefix.width, VecWidth::V128 | VecWidth::V256))
            || (prefix.encoding == VecEncodingKind::Evex
                && (prefix.v_high
                    || prefix.l_bits == 3
                    || prefix.b
                    || (prefix.zeroing && prefix.aaa == 0)
                    || (elem == VecElementType::F32 && prefix.w)
                    || (elem == VecElementType::F64 && !prefix.w)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let next_pc = pc + cursor as u64 + modrm.bytes_consumed as u64;
        let mut ops = Vec::new();
        let scalar_dd = elem == VecElementType::F64 && prefix.width == VecWidth::V128;
        let src = if modrm.is_memory && scalar_dd {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_disp8_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, 8, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            let scalar = ctx.alloc_vreg();
            let vector = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: scalar,
                    addr,
                    width: MemWidth::B8,
                    sign: SignExtend::Zero,
                },
            ));
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::VBroadcast {
                    dst: vector,
                    scalar,
                    elem,
                    lanes: 2,
                },
            ));
            vector
        } else if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
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
                modrm.rm
                    + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                        16
                    } else {
                        0
                    },
                prefix.width,
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
        let raw = if prefix.encoding == VecEncodingKind::Evex {
            ctx.alloc_vreg()
        } else {
            dst
        };
        self.append_duplicate_shuffle(raw, src, prefix.width, elem, high, pc, ctx, &mut ops);
        if prefix.encoding == VecEncodingKind::Evex {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed))
    }

    pub(crate) fn lift_vec_two_source_shuffle_imm(
        &self,
        prefix: VecPrefix,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let elem = match prefix.pp {
            X86SsePrefix::None => VecElementType::F32,
            X86SsePrefix::OpSize => VecElementType::F64,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        if (prefix.encoding == VecEncodingKind::Vex
            && !matches!(prefix.width, VecWidth::V128 | VecWidth::V256))
            || (prefix.encoding == VecEncodingKind::Evex
                && (prefix.l_bits == 3
                    || (prefix.zeroing && prefix.aaa == 0)
                    || (elem == VecElementType::F32 && prefix.w)
                    || (elem == VecElementType::F64 && !prefix.w)))
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        let broadcast = prefix.encoding == VecEncodingKind::Evex && prefix.b && modrm.is_memory;
        if prefix.b && !broadcast {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
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
            if broadcast {
                let scalar = ctx.alloc_vreg();
                let vector = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: if elem == VecElementType::F32 {
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
                        dst: vector,
                        scalar,
                        elem,
                        lanes: prefix.width.lanes(elem) as u8,
                    },
                ));
                vector
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
        let raw = if prefix.encoding == VecEncodingKind::Evex {
            ctx.alloc_vreg()
        } else {
            dst
        };
        self.append_two_source_shuffle_imm(
            raw,
            src1,
            src2,
            prefix.width,
            elem,
            bytes[imm_offset],
            pc,
            ctx,
            &mut ops,
        );
        if prefix.encoding == VecEncodingKind::Evex {
            self.append_evex_vector_mask_result(prefix, dst, raw, elem, pc, ctx, &mut ops);
        }
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_vec_extract_0f3a(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits != 0
            || prefix.vvvv != 0
            || prefix.v_high
            || prefix.aaa != 0
            || prefix.zeroing
            || prefix.b
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let (elem, lane_mask, mem_width, op_width) = match opcode {
            0x14 => (VecElementType::I8, 0x0F, MemWidth::B1, OpWidth::W32),
            0x15 => (VecElementType::I16, 0x07, MemWidth::B2, OpWidth::W32),
            0x16 if prefix.w => (VecElementType::I64, 0x01, MemWidth::B8, OpWidth::W64),
            0x16 => (VecElementType::I32, 0x03, MemWidth::B4, OpWidth::W32),
            0x17 => (VecElementType::I32, 0x03, MemWidth::B4, OpWidth::W32),
            _ => unreachable!(),
        };
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            rex: prefix.rex,
            operand_size_override: true,
            cursor,
            ..X86Prefix::default()
        };
        let modrm = decode_modrm(&bytes[cursor..], &modrm_prefix, pc)?;
        if prefix.encoding == VecEncodingKind::Evex && !modrm.is_memory && prefix.rm_high {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
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
        let addr = if modrm.is_memory {
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                mem_width.bytes(),
                ctx,
            );
            ops.extend(pre_ops);
            Some(addr)
        } else {
            None
        };
        let scalar = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VExtractLane {
                dst: scalar,
                vec: self.xmm(
                    modrm.reg
                        + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                            16
                        } else {
                            0
                        },
                ),
                lane: bytes[imm_offset] & lane_mask,
                elem,
                sign: SignExtend::Zero,
            },
        ));
        if let Some(addr) = addr {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Store {
                    src: scalar,
                    addr,
                    width: mem_width,
                },
            ));
        } else {
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Mov {
                    dst: self.gpr(modrm.rm),
                    src: SrcOperand::Reg(scalar),
                    width: op_width,
                },
            ));
        }
        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_vec_insert_0f3a(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits != 0
            || prefix.aaa != 0
            || prefix.zeroing
            || prefix.b
            || (prefix.encoding == VecEncodingKind::Evex && opcode == 0x21 && prefix.w)
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
        if prefix.encoding == VecEncodingKind::Evex
            && !modrm.is_memory
            && prefix.rm_high
            && opcode != 0x21
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let imm_offset = cursor + modrm.bytes_consumed;
        if bytes.len() <= imm_offset {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: imm_offset + 1,
            });
        }
        let imm = bytes[imm_offset];
        let next_pc = pc + imm_offset as u64 + 1;
        let mut ops = Vec::new();
        let dst = self.xmm(
            modrm.reg
                + if prefix.encoding == VecEncodingKind::Evex && prefix.reg_high {
                    16
                } else {
                    0
                },
        );
        let merge = self.xmm(
            prefix.vvvv
                + if prefix.encoding == VecEncodingKind::Evex && prefix.v_high {
                    16
                } else {
                    0
                },
        );

        if opcode == 0x21 {
            let inserted = if modrm.is_memory {
                let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    4,
                    ctx,
                );
                ops.extend(pre_ops);
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: MemWidth::B4,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            } else {
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::VExtractLane {
                        dst: scalar,
                        vec: self.xmm(
                            modrm.rm
                                + if prefix.encoding == VecEncodingKind::Evex && prefix.rm_high {
                                    16
                                } else {
                                    0
                                },
                        ),
                        lane: (imm >> 6) & 0x03,
                        elem: VecElementType::I32,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            };
            self.append_insertps(
                dst,
                merge,
                inserted,
                (imm >> 4) & 0x03,
                imm & 0x0F,
                pc,
                ctx,
                &mut ops,
            );
        } else {
            let (elem, lane_mask, mem_width) = match opcode {
                0x20 => (VecElementType::I8, 0x0F, MemWidth::B1),
                0x22 if prefix.w => (VecElementType::I64, 0x01, MemWidth::B8),
                0x22 => (VecElementType::I32, 0x03, MemWidth::B4),
                _ => unreachable!(),
            };
            let scalar = if modrm.is_memory {
                let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                    prefix,
                    modrm.addr.as_ref().unwrap(),
                    next_pc,
                    mem_width.bytes(),
                    ctx,
                );
                ops.extend(pre_ops);
                let scalar = ctx.alloc_vreg();
                ops.push(SmirOp::new(
                    OpId(ops.len() as u16),
                    pc,
                    OpKind::Load {
                        dst: scalar,
                        addr,
                        width: mem_width,
                        sign: SignExtend::Zero,
                    },
                ));
                scalar
            } else {
                self.gpr(modrm.rm)
            };
            self.append_insert_scalar_lane(
                dst,
                merge,
                scalar,
                elem,
                imm & lane_mask,
                pc,
                ctx,
                &mut ops,
            );
        }

        Ok(LiftResult::fallthrough(ops, imm_offset + 1))
    }

    pub(crate) fn lift_vec_palignr(
        &self,
        prefix: VecPrefix,
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
        let cursor = prefix.bytes + 1;
        let modrm_prefix = X86Prefix {
            operand_size_override: true,
            ..prefix.modrm_prefix(cursor)
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
        let imm = bytes[imm_offset];
        let next_pc = pc + imm_offset as u64 + 1;
        let lanes = prefix.width.lanes(VecElementType::I8) as u8;
        let mut ops = Vec::new();
        let low = if modrm.is_memory {
            let (addr, pre_ops) = if prefix.encoding == VecEncodingKind::Evex {
                self.vec_full_addr_to_smir(prefix, modrm.addr.as_ref().unwrap(), next_pc, ctx)
            } else {
                self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx)
            };
            ops.extend(pre_ops);
            let loaded = ctx.alloc_vreg();
            // Intel classifies EVEX VPALIGNR memory as Type E4NF.nb. The
            // complete Full Mem tuple is therefore read unconditionally even
            // when no output byte selects memory or every writemask bit is
            // clear.
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
        let high = self.vec_reg(
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
        if prefix.encoding == VecEncodingKind::Evex {
            let raw = ctx.alloc_vreg();
            self.append_align_right(raw, high, low, prefix.width, imm, pc, ctx, &mut ops);
            self.append_evex_vector_mask_result(
                prefix,
                dst,
                raw,
                VecElementType::I8,
                pc,
                ctx,
                &mut ops,
            );
        } else {
            self.append_align_right(dst, high, low, prefix.width, imm, pc, ctx, &mut ops);
        }
        Ok(self.retain_evex_memory_apx_requirement(
            &modrm,
            pc,
            LiftResult::fallthrough(ops, cursor + modrm.bytes_consumed + 1),
        ))
    }
}
