//! Memory-operation lowering

use crate::smir::lower::x86_64::*;
use std::collections::HashMap;

use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86AdxKind, X86AluEncoding, X86BlsKind, X86CacheControlKind, X86CountKind,
    X86OpHint, X86RepMode, X86SsePrefix, X86StringKind, X86VecAlign, X86VecMap, X86X87ControlKind,
};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, Condition, DispSize, FenceKind, FpRoundMode, GuestAddr, MemWidth,
    OpWidth, ShiftOp, SignExtend, SrcOperand, VLaneOp, VReg, VecCmpCond, VecElementType,
    VecUnaryOp, VecWidth, X86Reg,
};
use crate::smir::ir::{
    CallTarget, SmirBlock, SmirFunction, Terminator, X86InstructionBytes,
    x86_evex_native_replay_spans,
};

use crate::smir::lower::regalloc::{PhysReg, RegAlloc, RegLocation};
use crate::smir::lower::{
    CodeBuffer, LowerError, LowerResult, RelocKind, RelocTarget, Relocation, SmirLowerer,
    X86_GUEST_APX_ENABLED_OFFSET, X86_GUEST_CALL_FN_OFFSET, X86_GUEST_CPL_OFFSET,
    X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET, X86_GUEST_CTX_OFFSET, X86_GUEST_EXIT_PC_OFFSET,
    X86_GUEST_FS_BASE_OFFSET, X86_GUEST_GS_BASE_OFFSET, X86_GUEST_K_OFFSET,
    X86_GUEST_LOAD_FN_OFFSET, X86_GUEST_MXCSR_OFFSET, X86_GUEST_PAIR_LOAD_FN_OFFSET,
    X86_GUEST_PAIR_STORE_FN_OFFSET, X86_GUEST_RFLAGS_OFFSET, X86_GUEST_STORE_FN_OFFSET,
    X86_GUEST_TSC_AUX_OFFSET, X86_GUEST_VEC_LOAD_FN_OFFSET, X86_GUEST_VEC_STORE_FN_OFFSET,
    X86_GUEST_X87_TAG_WORD_OFFSET, X86_GUEST_XCR0_OFFSET, X86_GUEST_XGETBV1_OFFSET,
    X86_GUEST_ZMM_OFFSET, X86_HOST_MXCSR_OFFSET, X86_STATE_PTR_AT_RBP,
};

impl X86_64Lowerer {
    pub(crate) fn lower_op_memory(
        &mut self,
        op: &crate::smir::ir::ops::SmirOp,
    ) -> Result<(), LowerError> {
        let is_non_accumulating_madd = matches!(
            &op.kind,
            OpKind::VDotProduct {
                acc: VReg::Imm(0),
                mask: None,
                src_elem: VecElementType::I8,
                acc_elem: VecElementType::I16,
                src1_unsigned: true,
                saturate: true,
                zeroing: false,
                ..
            } | OpKind::VDotProduct {
                acc: VReg::Imm(0),
                mask: None,
                src_elem: VecElementType::I16,
                acc_elem: VecElementType::I32,
                src1_unsigned: false,
                saturate: false,
                zeroing: false,
                ..
            }
        );
        let is_classic_mpsadbw = matches!(
            (&op.kind, op.x86_hint),
            (
                OpKind::VMpsadbw { .. },
                Some(X86OpHint::SseOp { .. } | X86OpHint::VexOp { .. })
            )
        );
        if !is_non_accumulating_madd && !is_classic_mpsadbw {
            if let Some(result) = avx10::Avx10Lowerer::new().try_lower(&op.kind, &mut self.code) {
                return result;
            }
        }

        let alu_hint = match op.x86_hint {
            Some(X86OpHint::AluEncoding(enc)) => Some(enc),
            _ => None,
        };

        match &op.kind {
            // ================================================================
            // Memory Operations
            // ================================================================
            OpKind::VLoad { dst, addr, width } => {
                if self.mem_helpers {
                    return self.emit_jit_vector_or_mmx_mem_op(
                        op.guest_pc,
                        true,
                        *dst,
                        addr,
                        *width,
                        op.x86_hint,
                    );
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                if !dst_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VLoad".to_string(),
                        operand: "destination must be vector register".to_string(),
                    });
                }
                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding {
                            width: *width,
                            ..enc_hint
                        },
                        &[dst_reg],
                    );
                    self.emit_vec_mem(enc, dst_reg, None, addr)?;
                } else {
                    if *width != VecWidth::V128 || self.vec_requires_vex(&[dst_reg]) {
                        let enc = self.coerce_vec_encoding(
                            self.default_vec_mov_encoding(*width, 0x6F, op.x86_hint),
                            &[dst_reg],
                        );
                        self.emit_vec_mem(enc, dst_reg, None, addr)?;
                    } else {
                        let prefix = self.legacy_vec_move_prefix(op.x86_hint);
                        self.emit_sse_mov_mem(prefix, 0x6F, dst_reg, addr)?;
                    }
                }
            }

            OpKind::VStore { src, addr, width } => {
                if self.mem_helpers {
                    return self.emit_jit_vector_or_mmx_mem_op(
                        op.guest_pc,
                        false,
                        *src,
                        addr,
                        *width,
                        op.x86_hint,
                    );
                }
                let src_reg = self.get_reg(*src)?;
                if !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VStore".to_string(),
                        operand: "source must be vector register".to_string(),
                    });
                }
                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding {
                            width: *width,
                            ..enc_hint
                        },
                        &[src_reg],
                    );
                    self.emit_vec_mem(enc, src_reg, None, addr)?;
                } else {
                    if *width != VecWidth::V128 || self.vec_requires_vex(&[src_reg]) {
                        let enc = self.coerce_vec_encoding(
                            self.default_vec_mov_encoding(*width, 0x7F, op.x86_hint),
                            &[src_reg],
                        );
                        self.emit_vec_mem(enc, src_reg, None, addr)?;
                    } else {
                        let prefix = self.legacy_vec_move_prefix(op.x86_hint);
                        self.emit_sse_mov_mem(prefix, 0x7F, src_reg, addr)?;
                    }
                }
            }

            OpKind::VMov { dst, src, width } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VMov".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }
                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding {
                            width: *width,
                            ..enc_hint
                        },
                        &[dst_reg, src_reg],
                    );
                    let opcode = enc.opcode;
                    let (reg, rm) = if opcode == 0x7F || opcode == 0x29 {
                        (src_reg, dst_reg)
                    } else {
                        (dst_reg, src_reg)
                    };
                    self.emit_vec_rr(enc, reg, rm, 0);
                } else {
                    if *width != VecWidth::V128 || self.vec_requires_vex(&[dst_reg, src_reg]) {
                        let enc = self.coerce_vec_encoding(
                            self.default_vec_mov_encoding(*width, 0x6F, op.x86_hint),
                            &[dst_reg, src_reg],
                        );
                        self.emit_vec_rr(enc, dst_reg, src_reg, 0);
                    } else {
                        let prefix = self.legacy_vec_move_prefix(op.x86_hint);
                        let opcode = self.sse_opcode(op.x86_hint, 0x6F);
                        let (reg, rm) = if opcode == 0x7F {
                            (src_reg, dst_reg)
                        } else {
                            (dst_reg, src_reg)
                        };
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, opcode, reg, rm);
                    }
                }
            }

            OpKind::VAdd {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VAdd {:?}x{}", elem, lanes),
                    }
                })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VAdd".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding { width, ..enc_hint },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let (map, pp, opcode) = match elem {
                        VecElementType::I32 => (X86VecMap::Map0F, X86SsePrefix::OpSize, 0xFE),
                        VecElementType::F32 => (X86VecMap::Map0F, X86SsePrefix::None, 0x58),
                        VecElementType::F64 => (X86VecMap::Map0F, X86SsePrefix::OpSize, 0x58),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VAdd {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    let kind = if self.vec_requires_evex(width, &[dst_reg, src1_reg, src2_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    let enc = VecEncoding {
                        kind,
                        map,
                        pp,
                        opcode,
                        width,
                        w: *elem == VecElementType::F64,
                    };
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else {
                    let (prefix, opcode) = match elem {
                        VecElementType::I8 => (Some(0x66), 0xFC),
                        VecElementType::I16 => (Some(0x66), 0xFD),
                        VecElementType::I32 => (Some(0x66), 0xFE),
                        VecElementType::I64 => (Some(0x66), 0xD4),
                        VecElementType::F32 => (None, 0x58),
                        VecElementType::F64 => (Some(0x66), 0x58),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VAdd {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    if dst_reg != src1_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, 0x6F, dst_reg, src1_reg);
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src2_reg);
                }
            }

            OpKind::VSub {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VSub {:?}x{}", elem, lanes),
                    }
                })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VSub".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding { width, ..enc_hint },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let (map, pp, opcode) = match elem {
                        VecElementType::I32 => (X86VecMap::Map0F, X86SsePrefix::OpSize, 0xFA),
                        VecElementType::F32 => (X86VecMap::Map0F, X86SsePrefix::None, 0x5C),
                        VecElementType::F64 => (X86VecMap::Map0F, X86SsePrefix::OpSize, 0x5C),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VSub {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    let kind = if self.vec_requires_evex(width, &[dst_reg, src1_reg, src2_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    let enc = VecEncoding {
                        kind,
                        map,
                        pp,
                        opcode,
                        width,
                        w: *elem == VecElementType::F64,
                    };
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else {
                    let (prefix, opcode) = match elem {
                        VecElementType::I8 => (Some(0x66), 0xF8),
                        VecElementType::I16 => (Some(0x66), 0xF9),
                        VecElementType::I32 => (Some(0x66), 0xFA),
                        VecElementType::I64 => (Some(0x66), 0xFB),
                        VecElementType::F32 => (None, 0x5C),
                        VecElementType::F64 => (Some(0x66), 0x5C),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VSub {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    if dst_reg != src1_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, 0x6F, dst_reg, src1_reg);
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src2_reg);
                }
            }

            OpKind::VAddSubSat {
                dst,
                src1,
                src2,
                elem,
                lanes,
                subtract,
                signed,
            } => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!(
                            "VAddSubSat {:?}x{} subtract={} signed={}",
                            elem, lanes, subtract, signed
                        ),
                    }
                })?;
                let opcode = match (*elem, *subtract, *signed) {
                    (VecElementType::I8, false, true) => 0xEC,
                    (VecElementType::I16, false, true) => 0xED,
                    (VecElementType::I8, false, false) => 0xDC,
                    (VecElementType::I16, false, false) => 0xDD,
                    (VecElementType::I8, true, true) => 0xE8,
                    (VecElementType::I16, true, true) => 0xE9,
                    (VecElementType::I8, true, false) => 0xD8,
                    (VecElementType::I16, true, false) => 0xD9,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!(
                                "VAddSubSat {:?}x{} subtract={} signed={}",
                                elem, lanes, subtract, signed
                            ),
                        });
                    }
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VAddSubSat".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding { width, ..enc_hint },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let kind = if self.vec_requires_evex(width, &[dst_reg, src1_reg, src2_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    self.emit_vec_rrr(
                        VecEncoding {
                            kind,
                            map: X86VecMap::Map0F,
                            pp: X86SsePrefix::OpSize,
                            opcode,
                            width,
                            w: false,
                        },
                        dst_reg,
                        src1_reg,
                        src2_reg,
                    );
                } else {
                    let prefix = self.sse_prefix(op.x86_hint).or(Some(0x66));
                    let opcode = self.sse_opcode(op.x86_hint, opcode);
                    if dst_reg != src1_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, 0x6F, dst_reg, src1_reg);
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src2_reg);
                }
            }

            OpKind::VMax {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VMax {:?}x{}", elem, lanes),
                    }
                })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VMax".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding { width, ..enc_hint },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let (map, pp, opcode) = match elem {
                        VecElementType::F32 => (X86VecMap::Map0F, X86SsePrefix::None, 0x5F),
                        VecElementType::F64 => (X86VecMap::Map0F, X86SsePrefix::OpSize, 0x5F),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VMax {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    let kind = if self.vec_requires_evex(width, &[dst_reg, src1_reg, src2_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    let enc = VecEncoding {
                        kind,
                        map,
                        pp,
                        opcode,
                        width,
                        w: *elem == VecElementType::F64,
                    };
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else {
                    let (prefix, opcode) = match elem {
                        VecElementType::F32 => (None, 0x5F),
                        VecElementType::F64 => (Some(0x66), 0x5F),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VMax {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    if dst_reg != src1_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, 0x6F, dst_reg, src1_reg);
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src2_reg);
                }
            }

            OpKind::VX86MinMax {
                dst,
                src1,
                src2,
                elem,
                lanes,
                min,
            } => {
                let width =
                    if *lanes == 1 && matches!(elem, VecElementType::F32 | VecElementType::F64) {
                        VecWidth::V128
                    } else {
                        self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                            LowerError::UnsupportedOp {
                                op: format!("VX86MinMax {:?}x{}", elem, lanes),
                            }
                        })?
                    };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VX86MinMax".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                let opcode = if *min { 0x5D } else { 0x5F };
                let pp = match (*elem, *lanes == 1) {
                    (VecElementType::F32, false) => X86SsePrefix::None,
                    (VecElementType::F64, false) => X86SsePrefix::OpSize,
                    (VecElementType::F32, true) => X86SsePrefix::Rep,
                    (VecElementType::F64, true) => X86SsePrefix::Repne,
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("VX86MinMax {:?}x{}", elem, lanes),
                        });
                    }
                };

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding {
                            width,
                            opcode,
                            ..enc_hint
                        },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let kind = if self.vec_requires_evex(width, &[dst_reg, src1_reg, src2_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    self.emit_vec_rrr(
                        VecEncoding {
                            kind,
                            map: X86VecMap::Map0F,
                            pp,
                            opcode,
                            width,
                            w: *elem == VecElementType::F64,
                        },
                        dst_reg,
                        src1_reg,
                        src2_reg,
                    );
                } else {
                    if dst_reg != src1_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(Some(0xF3), 0x6F, dst_reg, src1_reg);
                    }
                    let prefix = match pp {
                        X86SsePrefix::None => None,
                        X86SsePrefix::OpSize => Some(0x66),
                        X86SsePrefix::Rep => Some(0xF3),
                        X86SsePrefix::Repne => Some(0xF2),
                    };
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src2_reg);
                }
            }

            OpKind::VDiv {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VDiv {:?}x{}", elem, lanes),
                    }
                })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VDiv".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding { width, ..enc_hint },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let pp = match elem {
                        VecElementType::F32 => X86SsePrefix::None,
                        VecElementType::F64 => X86SsePrefix::OpSize,
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VDiv {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    let kind = if self.vec_requires_evex(width, &[dst_reg, src1_reg, src2_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    self.emit_vec_rrr(
                        VecEncoding {
                            kind,
                            map: X86VecMap::Map0F,
                            pp,
                            opcode: 0x5E,
                            width,
                            w: *elem == VecElementType::F64,
                        },
                        dst_reg,
                        src1_reg,
                        src2_reg,
                    );
                } else {
                    let prefix = match elem {
                        VecElementType::F32 => None,
                        VecElementType::F64 => Some(0x66),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VDiv {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    if dst_reg != src1_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, 0x6F, dst_reg, src1_reg);
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, 0x5E, dst_reg, src2_reg);
                }
            }

            OpKind::VCmp {
                cond, elem, lanes, ..
            } if x86_state_vcmp_candidate(op) => {
                if !x86_state_vcmp_shape_valid(op) {
                    return Err(LowerError::InvalidOperand {
                        op: "state-backed VCmp".to_string(),
                        operand: format!(
                            "invalid XOP VPCOM vector compare {cond:?} {elem:?}x{lanes}"
                        ),
                    });
                }
                self.emit_x86_state_vcmp_op(op)?;
            }

            OpKind::VCmp {
                dst,
                src1,
                src2,
                cond,
                elem,
                lanes,
            } if matches!(
                (elem, cond),
                (
                    VecElementType::I8
                        | VecElementType::I16
                        | VecElementType::I32
                        | VecElementType::I64,
                    VecCmpCond::Eq | VecCmpCond::Gt
                )
            ) =>
            {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VCmp {:?} {:?}x{}", cond, elem, lanes),
                    }
                })?;
                if !matches!(width, VecWidth::V128 | VecWidth::V256) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("VCmp {:?} {:?}x{}", cond, elem, lanes),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index)) => index < 16,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VCmp".to_string(),
                        operand: "requires matching low vector registers".to_string(),
                    });
                }
                let (expected_map, expected_opcode) = match (*elem, *cond) {
                    (VecElementType::I8, VecCmpCond::Gt) => (X86VecMap::Map0F, 0x64),
                    (VecElementType::I16, VecCmpCond::Gt) => (X86VecMap::Map0F, 0x65),
                    (VecElementType::I32, VecCmpCond::Gt) => (X86VecMap::Map0F, 0x66),
                    (VecElementType::I8, VecCmpCond::Eq) => (X86VecMap::Map0F, 0x74),
                    (VecElementType::I16, VecCmpCond::Eq) => (X86VecMap::Map0F, 0x75),
                    (VecElementType::I32, VecCmpCond::Eq) => (X86VecMap::Map0F, 0x76),
                    (VecElementType::I64, VecCmpCond::Eq) => (X86VecMap::Map0F38, 0x29),
                    (VecElementType::I64, VecCmpCond::Gt) => (X86VecMap::Map0F38, 0x37),
                    _ => unreachable!(),
                };

                match op.x86_hint {
                    Some(X86OpHint::SseOp { prefix, opcode })
                        if width == VecWidth::V128
                            && dst_reg == src1_reg
                            && prefix == X86SsePrefix::OpSize
                            && opcode == expected_opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if expected_map == X86VecMap::Map0F38 {
                            emitter.emit_sse_op38_rr(Some(0x66), opcode, dst_reg, src2_reg);
                        } else {
                            emitter.emit_sse_mov_rr(Some(0x66), opcode, dst_reg, src2_reg);
                        }
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode,
                        width: encoded_width,
                        w,
                    }) if map == expected_map
                        && pp == X86SsePrefix::OpSize
                        && opcode == expected_opcode
                        && encoded_width == width =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!(
                                "unhinted or malformed VCmp {:?} {:?}x{}",
                                cond, elem, lanes
                            ),
                        });
                    }
                }
            }

            OpKind::VInterleave {
                dst,
                src1,
                src2,
                elem,
                lanes,
                block_lanes,
                high,
            } if matches!(
                elem,
                VecElementType::I8
                    | VecElementType::I16
                    | VecElementType::I32
                    | VecElementType::I64
            ) =>
            {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VInterleave {:?}x{}", elem, lanes),
                    }
                })?;
                if *block_lanes != (16 / elem.bytes()) as u8 {
                    return Err(LowerError::InvalidOperand {
                        op: "VInterleave".to_string(),
                        operand: "requires 128-bit lane blocks".to_string(),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VInterleave".to_string(),
                        operand: "requires matching vector registers".to_string(),
                    });
                }
                let opcode = match (*elem, *high) {
                    (VecElementType::I8, false) => 0x60,
                    (VecElementType::I16, false) => 0x61,
                    (VecElementType::I32, false) => 0x62,
                    (VecElementType::I64, false) => 0x6C,
                    (VecElementType::I8, true) => 0x68,
                    (VecElementType::I16, true) => 0x69,
                    (VecElementType::I32, true) => 0x6A,
                    (VecElementType::I64, true) => 0x6D,
                    _ => unreachable!(),
                };
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };

                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if width == VecWidth::V128
                        && dst_reg == src1_reg
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector)
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(Some(0x66), opcode, dst_reg, src2_reg);
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && width != VecWidth::V512
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && match elem {
                            VecElementType::I8 | VecElementType::I16 => true,
                            VecElementType::I32 => !w,
                            VecElementType::I64 => w,
                            _ => false,
                        } =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed VInterleave {:?}x{}", elem, lanes),
                        });
                    }
                }
            }

            OpKind::VPackSat {
                dst,
                src1,
                src2,
                src_elem,
                to_unsigned,
                src_lanes,
                block_lanes,
            } if matches!(src_elem, VecElementType::I16 | VecElementType::I32) => {
                let width = self
                    .vec_width_from_lanes(*src_elem, *src_lanes)
                    .ok_or_else(|| LowerError::UnsupportedOp {
                        op: format!("VPackSat {:?}x{}", src_elem, src_lanes),
                    })?;
                if *block_lanes != (16 / src_elem.bytes()) as u8 {
                    return Err(LowerError::InvalidOperand {
                        op: "VPackSat".to_string(),
                        operand: "requires 128-bit lane blocks".to_string(),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let r_m_reg = self.get_reg(*src1)?;
                let first_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, r_m_reg, first_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VPackSat".to_string(),
                        operand: "requires matching vector registers".to_string(),
                    });
                }
                let (map, opcode) = match (*src_elem, *to_unsigned) {
                    (VecElementType::I16, false) => (X86VecMap::Map0F, 0x63),
                    (VecElementType::I16, true) => (X86VecMap::Map0F, 0x67),
                    (VecElementType::I32, false) => (X86VecMap::Map0F, 0x6B),
                    (VecElementType::I32, true) => (X86VecMap::Map0F38, 0x2B),
                    _ => unreachable!(),
                };
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if width == VecWidth::V128
                        && dst_reg == first_reg
                        && [dst_reg, r_m_reg, first_reg].into_iter().all(low_vector)
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if map == X86VecMap::Map0F38 {
                            emitter.emit_sse_op38_rr(Some(0x66), opcode, dst_reg, r_m_reg);
                        } else {
                            emitter.emit_sse_mov_rr(Some(0x66), opcode, dst_reg, r_m_reg);
                        }
                    }
                    Some(X86OpHint::VexOp {
                        map: encoded_map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if encoded_map == map
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && width != VecWidth::V512
                        && [dst_reg, r_m_reg, first_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            first_reg,
                            r_m_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map: encoded_map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if encoded_map == map
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && (*src_elem == VecElementType::I16 || !w) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            first_reg,
                            r_m_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!(
                                "unhinted or malformed VPackSat {:?}x{}",
                                src_elem, src_lanes
                            ),
                        });
                    }
                }
            }

            OpKind::VByteShuffle {
                dst,
                src,
                control,
                lanes,
                block_lanes,
            } => {
                let width = self
                    .vec_width_from_lanes(VecElementType::I8, *lanes)
                    .ok_or_else(|| LowerError::UnsupportedOp {
                        op: format!("VByteShuffle I8x{lanes}"),
                    })?;
                if *block_lanes != 16 {
                    return Err(LowerError::InvalidOperand {
                        op: "VByteShuffle".to_string(),
                        operand: "requires 16-byte lane blocks".to_string(),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let control_reg = self.get_reg(*control)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src_reg, control_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VByteShuffle".to_string(),
                        operand: "requires matching vector registers".to_string(),
                    });
                }
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp { prefix, opcode })
                        if width == VecWidth::V128
                            && dst_reg == src_reg
                            && [dst_reg, src_reg, control_reg].into_iter().all(low_vector)
                            && prefix == X86SsePrefix::OpSize
                            && opcode == 0x00 =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_op38_rr(Some(0x66), 0x00, dst_reg, control_reg);
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F38
                        && pp == X86SsePrefix::OpSize
                        && opcode == 0x00
                        && encoded_width == width
                        && width != VecWidth::V512
                        && [dst_reg, src_reg, control_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src_reg,
                            control_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map,
                        pp,
                        opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F38
                        && pp == X86SsePrefix::OpSize
                        && opcode == 0x00
                        && encoded_width == width =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src_reg,
                            control_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed VByteShuffle I8x{lanes}"),
                        });
                    }
                }
            }

            OpKind::VHorizontalBin {
                dst,
                src1,
                src2,
                elem,
                lanes,
                block_lanes,
                subtract,
                saturating,
            } => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VHorizontalBin {:?}x{}", elem, lanes),
                    }
                })?;
                if !matches!(elem, VecElementType::I16 | VecElementType::I32)
                    || *block_lanes != (16 / elem.bytes()) as u8
                    || (*saturating && *elem != VecElementType::I16)
                    || width == VecWidth::V512
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VHorizontalBin".to_string(),
                        operand: "requires exact 128-bit I16/I32 lane blocks".to_string(),
                    });
                }
                let opcode = match (elem, subtract, saturating) {
                    (VecElementType::I16, false, false) => 0x01,
                    (VecElementType::I32, false, false) => 0x02,
                    (VecElementType::I16, false, true) => 0x03,
                    (VecElementType::I16, true, false) => 0x05,
                    (VecElementType::I32, true, false) => 0x06,
                    (VecElementType::I16, true, true) => 0x07,
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "VHorizontalBin".to_string(),
                            operand: "unsupported element/mode combination".to_string(),
                        });
                    }
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VHorizontalBin".to_string(),
                        operand: "requires matching XMM/YMM registers".to_string(),
                    });
                }
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if width == VecWidth::V128
                        && dst_reg == src1_reg
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector)
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_op38_rr(Some(0x66), opcode, dst_reg, src2_reg);
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F38
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!(
                                "unhinted or malformed VHorizontalBin {:?}x{}",
                                elem, lanes
                            ),
                        });
                    }
                }
            }

            OpKind::VMulShiftSat {
                dst,
                src1,
                src2,
                src_elem,
                lanes,
                signed1,
                signed2,
                shift_left,
                round,
                sat_bits,
                out_shift,
            } => {
                if *src_elem != VecElementType::I16 || *shift_left != 0 || *sat_bits != 0 {
                    return Err(LowerError::InvalidOperand {
                        op: "VMulShiftSat PMULH[RU]SW".to_string(),
                        operand: "requires I16 multiply, zero left shift, and no saturation"
                            .to_string(),
                    });
                }
                let (expected_map, expected_opcode, mnemonic) = match (
                    *signed1, *signed2, *round, *out_shift,
                ) {
                    (true, true, true, 15) => (X86VecMap::Map0F38, 0x0B, "PMULHRSW"),
                    (true, true, false, 16) => (X86VecMap::Map0F, 0xE5, "PMULHW"),
                    (false, false, false, 16) => (X86VecMap::Map0F, 0xE4, "PMULHUW"),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "VMulShiftSat PMULH[RU]SW".to_string(),
                            operand: "requires signed rounded >>15, signed >>16, or unsigned >>16 semantics"
                                .to_string(),
                        });
                    }
                };
                let width = self
                    .vec_width_from_lanes(VecElementType::I16, *lanes)
                    .ok_or_else(|| LowerError::UnsupportedOp {
                        op: format!("VMulShiftSat {mnemonic} I16x{lanes}"),
                    })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: format!("VMulShiftSat {mnemonic}"),
                        operand: "requires matching XMM/YMM/ZMM registers".to_string(),
                    });
                }
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if width == VecWidth::V128
                        && dst_reg == src1_reg
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector)
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == expected_opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if expected_map == X86VecMap::Map0F38 {
                            emitter.emit_sse_op38_rr(
                                Some(0x66),
                                expected_opcode,
                                dst_reg,
                                src2_reg,
                            );
                        } else {
                            emitter.emit_sse_mov_rr(Some(0x66), expected_opcode, dst_reg, src2_reg);
                        }
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w: _,
                    }) if map == expected_map
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == expected_opcode
                        && encoded_width == width
                        && width != VecWidth::V512
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode: expected_opcode,
                                width,
                                // The PMULH[RU]SW family is WIG. Canonicalize the native
                                // encoding instead of replaying a noncanonical
                                // guest W=1 payload on the host.
                                w: false,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w: _,
                    }) if map == expected_map
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == expected_opcode
                        && encoded_width == width =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode: expected_opcode,
                                width,
                                // The PMULH[RU]SW family is WIG; use the canonical host
                                // encoding for both guest W values.
                                w: false,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed {mnemonic} {width:?}"),
                        });
                    }
                }
            }

            OpKind::VLane {
                dst,
                src1,
                src2,
                elem,
                lanes,
                op: lane_op @ (VLaneOp::Min | VLaneOp::Max),
                signed,
                set_ovf: false,
            } if matches!(
                elem,
                VecElementType::I8
                    | VecElementType::I16
                    | VecElementType::I32
                    | VecElementType::I64
            ) =>
            {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VLane {:?} {:?}x{}", lane_op, elem, lanes),
                    }
                })?;
                let (map, opcode) = match (*elem, *lane_op, *signed) {
                    (VecElementType::I8, VLaneOp::Min, false) => (X86VecMap::Map0F, 0xDA),
                    (VecElementType::I8, VLaneOp::Max, false) => (X86VecMap::Map0F, 0xDE),
                    (VecElementType::I16, VLaneOp::Min, true) => (X86VecMap::Map0F, 0xEA),
                    (VecElementType::I16, VLaneOp::Max, true) => (X86VecMap::Map0F, 0xEE),
                    (VecElementType::I8, VLaneOp::Min, true) => (X86VecMap::Map0F38, 0x38),
                    (VecElementType::I32 | VecElementType::I64, VLaneOp::Min, true) => {
                        (X86VecMap::Map0F38, 0x39)
                    }
                    (VecElementType::I16, VLaneOp::Min, false) => (X86VecMap::Map0F38, 0x3A),
                    (VecElementType::I32 | VecElementType::I64, VLaneOp::Min, false) => {
                        (X86VecMap::Map0F38, 0x3B)
                    }
                    (VecElementType::I8, VLaneOp::Max, true) => (X86VecMap::Map0F38, 0x3C),
                    (VecElementType::I32 | VecElementType::I64, VLaneOp::Max, true) => {
                        (X86VecMap::Map0F38, 0x3D)
                    }
                    (VecElementType::I16, VLaneOp::Max, false) => (X86VecMap::Map0F38, 0x3E),
                    (VecElementType::I32 | VecElementType::I64, VLaneOp::Max, false) => {
                        (X86VecMap::Map0F38, 0x3F)
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("VLane {:?} {:?}x{}", lane_op, elem, lanes),
                        });
                    }
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VLane packed integer min/max".to_string(),
                        operand: "requires matching XMM/YMM/ZMM registers".to_string(),
                    });
                }
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if *elem != VecElementType::I64
                        && width == VecWidth::V128
                        && dst_reg == src1_reg
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector)
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if map == X86VecMap::Map0F38 {
                            emitter.emit_sse_op38_rr(Some(0x66), opcode, dst_reg, src2_reg);
                        } else {
                            emitter.emit_sse_mov_rr(Some(0x66), opcode, dst_reg, src2_reg);
                        }
                    }
                    Some(X86OpHint::VexOp {
                        map: encoded_map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w: _,
                    }) if *elem != VecElementType::I64
                        && encoded_map == map
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && width != VecWidth::V512
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                // All packed-integer min/max VEX encodings are WIG.
                                w: false,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map: encoded_map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if encoded_map == map
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && match elem {
                            VecElementType::I8 | VecElementType::I16 => true,
                            VecElementType::I32 => !w,
                            VecElementType::I64 => w,
                            _ => false,
                        } =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode,
                                width,
                                // EVEX byte/word W is ignored; dword/qword use W0/W1.
                                w: *elem == VecElementType::I64,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!(
                                "unhinted or malformed packed integer {:?} {:?}x{}",
                                lane_op, elem, lanes
                            ),
                        });
                    }
                }
            }

            OpKind::VLane {
                dst,
                src1,
                src2,
                elem,
                lanes,
                op: VLaneOp::Sign,
                signed: true,
                set_ovf: false,
            } if matches!(
                elem,
                VecElementType::I8 | VecElementType::I16 | VecElementType::I32
            ) =>
            {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VLane Sign {:?}x{}", elem, lanes),
                    }
                })?;
                if !matches!(width, VecWidth::V128 | VecWidth::V256) {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("VLane Sign {:?}x{}", elem, lanes),
                    });
                }
                let opcode = match elem {
                    VecElementType::I8 => 0x08,
                    VecElementType::I16 => 0x09,
                    VecElementType::I32 => 0x0A,
                    _ => unreachable!("guarded PSIGN element width"),
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let low_vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index)) => index < 16,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(low_vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VLane Sign PSIGN[BWD]".to_string(),
                        operand: "requires matching low XMM/YMM registers".to_string(),
                    });
                }
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if width == VecWidth::V128
                        && dst_reg == src1_reg
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_op38_rr(Some(0x66), opcode, dst_reg, src2_reg);
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w: _,
                    }) if map == X86VecMap::Map0F38
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                // VPSIGNB/W/D are WIG. Canonicalize guest W=1 to W=0
                                // instead of replaying a noncanonical host encoding.
                                w: false,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed PSIGN[BWD] {width:?}"),
                        });
                    }
                }
            }

            OpKind::VLane {
                dst,
                src1,
                src2,
                elem,
                lanes,
                op: VLaneOp::AvgRnd,
                signed: false,
                set_ovf: false,
            } if matches!(elem, VecElementType::I8 | VecElementType::I16) => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VLane AvgRnd {:?}x{}", elem, lanes),
                    }
                })?;
                let opcode = match elem {
                    VecElementType::I8 => 0xE0,
                    VecElementType::I16 => 0xE3,
                    _ => unreachable!("guarded PAVG element width"),
                };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VLane AvgRnd PAVG[BW]".to_string(),
                        operand: "requires matching XMM/YMM/ZMM registers".to_string(),
                    });
                }
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if width == VecWidth::V128
                        && dst_reg == src1_reg
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector)
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(Some(0x66), opcode, dst_reg, src2_reg);
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width
                        && width != VecWidth::V512
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == opcode
                        && encoded_width == width =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode,
                                width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed PAVG[BW] {width:?}"),
                        });
                    }
                }
            }

            OpKind::VSadBytes {
                dst,
                src1,
                src2,
                width,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VSadBytes PSADBW".to_string(),
                        operand: "requires matching XMM/YMM/ZMM registers".to_string(),
                    });
                }
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if *width == VecWidth::V128
                        && dst_reg == src1_reg
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector)
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == 0xF6 =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(Some(0x66), 0xF6, dst_reg, src2_reg);
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == 0xF6
                        && encoded_width == *width
                        && *width != VecWidth::V512
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode: 0xF6,
                                width: *width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == 0xF6
                        && encoded_width == *width =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode: 0xF6,
                                width: *width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed PSADBW {width:?}"),
                        });
                    }
                }
            }

            OpKind::X86Phminposuw { dst, src } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let low_xmm = |reg: PhysReg| matches!(reg, PhysReg::Xmm(0..=15));
                if !low_xmm(dst_reg) || !low_xmm(src_reg) {
                    return Err(LowerError::InvalidOperand {
                        op: "X86Phminposuw".to_string(),
                        operand: "requires low XMM registers".to_string(),
                    });
                }
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::OpSize,
                        opcode: 0x41,
                    }) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_op38_rr(Some(0x66), 0x41, dst_reg, src_reg);
                    }
                    Some(X86OpHint::VexOp {
                        map: X86VecMap::Map0F38,
                        pp: X86SsePrefix::OpSize,
                        opcode: 0x41,
                        width: VecWidth::V128,
                        ..
                    }) => {
                        // VEX.W is ignored architecturally; emit canonical W0.
                        // vvvv=0 is encoded inverted as the required 1111b.
                        self.emit_vec_rr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map: X86VecMap::Map0F38,
                                pp: X86SsePrefix::OpSize,
                                opcode: 0x41,
                                width: VecWidth::V128,
                                w: false,
                            },
                            dst_reg,
                            src_reg,
                            0,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "unhinted or malformed PHMINPOSUW".to_string(),
                        });
                    }
                }
            }

            OpKind::X86MovMask {
                dst,
                src,
                elem,
                lanes,
                dst_width,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let valid_gpr = matches!(
                    dst_reg,
                    PhysReg::Rax
                        | PhysReg::Rcx
                        | PhysReg::Rdx
                        | PhysReg::Rbx
                        | PhysReg::Rsi
                        | PhysReg::Rdi
                        | PhysReg::R8
                        | PhysReg::R9
                        | PhysReg::R10
                        | PhysReg::R11
                        | PhysReg::R12
                        | PhysReg::R13
                        | PhysReg::R14
                        | PhysReg::R15
                );
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::InvalidOperand {
                        op: "X86MovMask".to_string(),
                        operand: format!("invalid {elem:?} lane count {lanes}"),
                    }
                })?;
                let valid_source = matches!(
                    (width, src_reg),
                    (VecWidth::V128, PhysReg::Xmm(0..=15)) | (VecWidth::V256, PhysReg::Ymm(0..=15))
                );
                if !valid_gpr
                    || !valid_source
                    || !matches!(
                        (elem, lanes),
                        (VecElementType::I8, 16 | 32)
                            | (VecElementType::F32, 4 | 8)
                            | (VecElementType::F64, 2 | 4)
                    )
                    || !matches!(dst_width, OpWidth::W32 | OpWidth::W64)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "X86MovMask".to_string(),
                        operand: "requires a safe legacy GPR and matching low XMM/YMM source"
                            .to_string(),
                    });
                }
                let encoding_matches = |opcode: u8, pp: X86SsePrefix| match (opcode, pp, elem) {
                    (0x50, X86SsePrefix::None, VecElementType::F32)
                    | (0x50, X86SsePrefix::OpSize, VecElementType::F64)
                    | (0xD7, X86SsePrefix::OpSize, VecElementType::I8) => true,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp { prefix, opcode })
                        if width == VecWidth::V128 && encoding_matches(opcode, prefix) =>
                    {
                        let legacy_prefix = match prefix {
                            X86SsePrefix::None => None,
                            X86SsePrefix::OpSize => Some(0x66),
                            _ => unreachable!("validated MOVMSK legacy prefix"),
                        };
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_mask_rr(
                            legacy_prefix,
                            opcode,
                            dst_reg,
                            src_reg,
                            *dst_width == OpWidth::W64,
                        );
                    }
                    Some(X86OpHint::VexOp {
                        map: X86VecMap::Map0F,
                        pp,
                        opcode,
                        width: encoded_width,
                        ..
                    }) if *dst_width == OpWidth::W32
                        && encoded_width == width
                        && width != VecWidth::V512
                        && encoding_matches(opcode, pp) =>
                    {
                        // Every family member is WIG; emit canonical VEX.W0.
                        // vvvv=0 becomes the required encoded 1111b.
                        self.emit_vec_rr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map: X86VecMap::Map0F,
                                pp,
                                opcode,
                                width,
                                w: false,
                            },
                            dst_reg,
                            src_reg,
                            0,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "unhinted or malformed MOVMSK/PMOVMSKB".to_string(),
                        });
                    }
                }
            }

            OpKind::X86MovdQ {
                dst,
                src,
                width,
                zero_upper,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let safe_gpr = |reg: PhysReg| {
                    matches!(
                        reg,
                        PhysReg::Rax
                            | PhysReg::Rcx
                            | PhysReg::Rdx
                            | PhysReg::Rbx
                            | PhysReg::Rsi
                            | PhysReg::Rdi
                            | PhysReg::R8
                            | PhysReg::R9
                            | PhysReg::R10
                            | PhysReg::R11
                            | PhysReg::R12
                            | PhysReg::R13
                            | PhysReg::R14
                            | PhysReg::R15
                    )
                };
                let (xmm, gpr, vector_dst) = match (dst_reg, src_reg) {
                    (xmm @ PhysReg::Xmm(0..=31), gpr) if safe_gpr(gpr) => (xmm, gpr, true),
                    (gpr, xmm @ PhysReg::Xmm(0..=31)) if safe_gpr(gpr) => (xmm, gpr, false),
                    _ => {
                        return Err(LowerError::InvalidOperand {
                            op: "X86MovdQ".to_string(),
                            operand: "requires one safe GPR and one XMM register".to_string(),
                        });
                    }
                };
                if !matches!(width, OpWidth::W32 | OpWidth::W64) {
                    return Err(LowerError::InvalidOperand {
                        op: "X86MovdQ".to_string(),
                        operand: "width must be 32 or 64 bits".to_string(),
                    });
                }
                let expected_opcode = if vector_dst { 0x6E } else { 0x7E };
                match op.x86_hint {
                    Some(X86OpHint::SseOp { prefix, opcode })
                        if prefix == X86SsePrefix::OpSize
                            && opcode == expected_opcode
                            && xmm.encoding() < 16
                            && !*zero_upper =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_movd_q_rr(opcode, xmm, gpr, *width);
                    }
                    Some(X86OpHint::VexOp {
                        map: X86VecMap::Map0F,
                        pp: X86SsePrefix::OpSize,
                        opcode,
                        width: VecWidth::V128,
                        w,
                    }) if opcode == expected_opcode
                        && w == (*width == OpWidth::W64)
                        && xmm.encoding() < 16
                        && *zero_upper == vector_dst =>
                    {
                        self.emit_vec_rr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map: X86VecMap::Map0F,
                                pp: X86SsePrefix::OpSize,
                                opcode,
                                width: VecWidth::V128,
                                w,
                            },
                            xmm,
                            gpr,
                            0,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map: X86VecMap::Map0F,
                        pp: X86SsePrefix::OpSize,
                        opcode,
                        width: VecWidth::V128,
                        w,
                    }) if opcode == expected_opcode
                        && w == (*width == OpWidth::W64)
                        && *zero_upper == vector_dst =>
                    {
                        self.emit_vec_rr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map: X86VecMap::Map0F,
                                pp: X86SsePrefix::OpSize,
                                opcode,
                                width: VecWidth::V128,
                                w,
                            },
                            xmm,
                            gpr,
                            0,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "unhinted or malformed MOVD/MOVQ".to_string(),
                        });
                    }
                }
            }

            OpKind::VMpsadbw {
                dst,
                src1,
                src2,
                mask,
                width,
                imm,
                zeroing,
            } => {
                if mask.is_some() || *zeroing {
                    return Err(LowerError::UnsupportedOp {
                        op: "masked AVX10.2 VMPSADBW requires EVEX lowering".to_string(),
                    });
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index)) => index < 16,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: "VMpsadbw MPSADBW".to_string(),
                        operand: "requires matching low XMM/YMM registers".to_string(),
                    });
                }
                match op.x86_hint {
                    Some(X86OpHint::SseOp {
                        prefix,
                        opcode: encoded_opcode,
                    }) if *width == VecWidth::V128
                        && dst_reg == src1_reg
                        && prefix == X86SsePrefix::OpSize
                        && encoded_opcode == 0x42 =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_op3a_rr_imm(Some(0x66), 0x42, dst_reg, src2_reg, *imm);
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode: encoded_opcode,
                        width: encoded_width,
                        w,
                    }) if map == X86VecMap::Map0F3A
                        && pp == X86SsePrefix::OpSize
                        && encoded_opcode == 0x42
                        && encoded_width == *width =>
                    {
                        self.emit_vec_rrr_imm(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode: 0x42,
                                width: *width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                            *imm,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed MPSADBW {width:?}"),
                        });
                    }
                }
            }

            OpKind::VDotProduct {
                dst,
                acc: VReg::Imm(0),
                src1,
                src2,
                mask: None,
                src_elem,
                acc_elem,
                width,
                src1_unsigned,
                saturate,
                zeroing: false,
            } if matches!(
                (src_elem, acc_elem, src1_unsigned, saturate),
                (VecElementType::I8, VecElementType::I16, true, true)
                    | (VecElementType::I16, VecElementType::I32, false, false)
            ) =>
            {
                let maddubs = *src_elem == VecElementType::I8;
                let instruction = if maddubs { "PMADDUBSW" } else { "PMADDWD" };
                let expected_map = if maddubs {
                    X86VecMap::Map0F38
                } else {
                    X86VecMap::Map0F
                };
                let expected_opcode = if maddubs { 0x04 } else { 0xF5 };
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let vector_matches_width = |reg: PhysReg| match (width, reg) {
                    (VecWidth::V128, PhysReg::Xmm(index))
                    | (VecWidth::V256, PhysReg::Ymm(index))
                    | (VecWidth::V512, PhysReg::Zmm(index)) => index < 32,
                    _ => false,
                };
                if ![dst_reg, src1_reg, src2_reg]
                    .into_iter()
                    .all(vector_matches_width)
                {
                    return Err(LowerError::InvalidOperand {
                        op: format!("VDotProduct {instruction}"),
                        operand: "requires matching vector registers".to_string(),
                    });
                }
                let low_vector = |reg: PhysReg| match reg {
                    PhysReg::Xmm(index) | PhysReg::Ymm(index) | PhysReg::Zmm(index) => index < 16,
                    _ => false,
                };
                match op.x86_hint {
                    Some(X86OpHint::SseOp { prefix, opcode })
                        if *width == VecWidth::V128
                            && dst_reg == src1_reg
                            && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector)
                            && prefix == X86SsePrefix::OpSize
                            && opcode == expected_opcode =>
                    {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        if maddubs {
                            emitter.emit_sse_op38_rr(
                                Some(0x66),
                                expected_opcode,
                                dst_reg,
                                src2_reg,
                            );
                        } else {
                            emitter.emit_sse_mov_rr(Some(0x66), expected_opcode, dst_reg, src2_reg);
                        }
                    }
                    Some(X86OpHint::VexOp {
                        map,
                        pp,
                        opcode,
                        width: encoded_width,
                        w,
                    }) if map == expected_map
                        && pp == X86SsePrefix::OpSize
                        && opcode == expected_opcode
                        && encoded_width == *width
                        && *width != VecWidth::V512
                        && [dst_reg, src1_reg, src2_reg].into_iter().all(low_vector) =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Vex,
                                map,
                                pp,
                                opcode,
                                width: *width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    Some(X86OpHint::EvexOp {
                        map,
                        pp,
                        opcode,
                        width: encoded_width,
                        w,
                    }) if map == expected_map
                        && pp == X86SsePrefix::OpSize
                        && opcode == expected_opcode
                        && encoded_width == *width =>
                    {
                        self.emit_vec_rrr(
                            VecEncoding {
                                kind: VecEncodingKind::Evex,
                                map,
                                pp,
                                opcode,
                                width: *width,
                                w,
                            },
                            dst_reg,
                            src1_reg,
                            src2_reg,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("unhinted or malformed {instruction} {width:?}"),
                        });
                    }
                }
            }

            OpKind::VUnary {
                dst,
                src,
                elem,
                lanes,
                op: VecUnaryOp::Abs,
            } if matches!(
                elem,
                VecElementType::I8
                    | VecElementType::I16
                    | VecElementType::I32
                    | VecElementType::I64
            ) =>
            {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VUnary Abs {:?}x{}", elem, lanes),
                    }
                })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VUnary Abs".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc) = self.vec_hint(op.x86_hint) {
                    self.emit_vec_rr(VecEncoding { width, ..enc }, dst_reg, src_reg, 0);
                } else if matches!(op.x86_hint, Some(X86OpHint::SseOp { .. })) {
                    if *elem == VecElementType::I64 || width != VecWidth::V128 {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("legacy VUnary Abs {:?}x{}", elem, lanes),
                        });
                    }
                    let prefix = self.sse_prefix(op.x86_hint).or(Some(0x66));
                    let opcode = self.sse_opcode(
                        op.x86_hint,
                        match elem {
                            VecElementType::I8 => 0x1C,
                            VecElementType::I16 => 0x1D,
                            VecElementType::I32 => 0x1E,
                            _ => unreachable!(),
                        },
                    );
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_op38_rr(prefix, opcode, dst_reg, src_reg);
                } else {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("unhinted VUnary Abs {:?}x{}", elem, lanes),
                    });
                }
            }

            OpKind::VUnary {
                elem, lanes, op, ..
            } => {
                return Err(LowerError::UnsupportedOp {
                    op: format!("VUnary {:?} {:?}x{} (x86)", op, elem, lanes),
                });
            }

            OpKind::VReduce {
                elem, lanes, op, ..
            } => {
                // Vector across-lanes reduction (ADDV/SMAXV/…) is emitted only
                // by the AArch64 lifter; not implemented in the x86 lowerer.
                return Err(LowerError::UnsupportedOp {
                    op: format!("VReduce {:?} {:?}x{} (x86)", op, elem, lanes),
                });
            }

            OpKind::VFMinMaxNm { elem, lanes, .. } => {
                // FP numeric min/max (FMAXNM/FMINNM) is emitted only by the
                // AArch64 lifter; not implemented in the x86 lowerer.
                return Err(LowerError::UnsupportedOp {
                    op: format!("VFMinMaxNm {:?}x{} (x86)", elem, lanes),
                });
            }

            OpKind::VPermute2 {
                elem, lanes, kind, ..
            } => {
                // Vector permute (ZIP/UZP/TRN) is emitted only by the AArch64
                // lifter; not implemented in the x86 lowerer.
                return Err(LowerError::UnsupportedOp {
                    op: format!("VPermute2 {:?} {:?}x{} (x86)", kind, elem, lanes),
                });
            }

            OpKind::VTableLookup {
                num_tables, lanes, ..
            } => {
                // Vector table lookup (TBL/TBX) is emitted only by the AArch64
                // lifter; not implemented in the x86 lowerer.
                return Err(LowerError::UnsupportedOp {
                    op: format!("VTableLookup {num_tables}-table x{lanes} (x86)"),
                });
            }

            OpKind::VMul {
                dst,
                src1,
                src2,
                elem,
                lanes,
            } => {
                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VMul {:?}x{}", elem, lanes),
                    }
                })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VMul".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding { width, ..enc_hint },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if *elem == VecElementType::I64
                    || width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let (map, pp, opcode) = match elem {
                        VecElementType::I16 => (X86VecMap::Map0F, X86SsePrefix::OpSize, 0xD5),
                        VecElementType::I32 => (X86VecMap::Map0F38, X86SsePrefix::OpSize, 0x40),
                        VecElementType::I64 => (X86VecMap::Map0F38, X86SsePrefix::OpSize, 0x40),
                        VecElementType::F32 => (X86VecMap::Map0F, X86SsePrefix::None, 0x59),
                        VecElementType::F64 => (X86VecMap::Map0F, X86SsePrefix::OpSize, 0x59),
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VMul {:?}x{}", elem, lanes),
                            });
                        }
                    };
                    let kind = if *elem == VecElementType::I64
                        || self.vec_requires_evex(width, &[dst_reg, src1_reg, src2_reg])
                    {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    let enc = VecEncoding {
                        kind,
                        map,
                        pp,
                        opcode,
                        width,
                        w: matches!(elem, VecElementType::I64 | VecElementType::F64),
                    };
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else {
                    match elem {
                        VecElementType::I16 => {
                            if dst_reg != src1_reg {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_sse_mov_rr(Some(0x66), 0x6F, dst_reg, src1_reg);
                            }
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_sse_mov_rr(Some(0x66), 0xD5, dst_reg, src2_reg);
                        }
                        VecElementType::I32 => {
                            if dst_reg != src1_reg {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_sse_mov_rr(Some(0x66), 0x6F, dst_reg, src1_reg);
                            }
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_sse_op38_rr(Some(0x66), 0x40, dst_reg, src2_reg);
                        }
                        VecElementType::F32 | VecElementType::F64 => {
                            let prefix = if matches!(elem, VecElementType::F64) {
                                Some(0x66)
                            } else {
                                None
                            };
                            if dst_reg != src1_reg {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_sse_mov_rr(prefix, 0x6F, dst_reg, src1_reg);
                            }
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_sse_mov_rr(prefix, 0x59, dst_reg, src2_reg);
                        }
                        _ => {
                            return Err(LowerError::UnsupportedOp {
                                op: format!("VMul {:?}x{}", elem, lanes),
                            });
                        }
                    }
                }
            }

            OpKind::VAnd {
                dst,
                src1,
                src2,
                width,
            }
            | OpKind::VAndNot {
                dst,
                src1,
                src2,
                width,
            }
            | OpKind::VOr {
                dst,
                src1,
                src2,
                width,
            }
            | OpKind::VXor {
                dst,
                src1,
                src2,
                width,
            } => {
                let dst_reg = self.get_dst_reg(*dst)?;
                let src1_reg = self.get_reg(*src1)?;
                let src2_reg = self.get_reg(*src2)?;
                let default_opcode = match &op.kind {
                    OpKind::VAnd { .. } => 0x54,
                    OpKind::VAndNot { .. } => 0x55,
                    OpKind::VOr { .. } => 0x56,
                    OpKind::VXor { .. } => 0x57,
                    _ => unreachable!(),
                };
                if !dst_reg.is_vec() || !src1_reg.is_vec() || !src2_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "vector logic".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                } else if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding {
                            width: *width,
                            ..enc_hint
                        },
                        &[dst_reg, src1_reg, src2_reg],
                    );
                    self.emit_vec_rrr(enc, dst_reg, src1_reg, src2_reg);
                } else if *width != VecWidth::V128
                    || self.vec_requires_vex(&[dst_reg, src1_reg, src2_reg])
                {
                    let kind = if self.vec_requires_evex(*width, &[dst_reg, src1_reg, src2_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    self.emit_vec_rrr(
                        VecEncoding {
                            kind,
                            map: X86VecMap::Map0F,
                            pp: X86SsePrefix::None,
                            opcode: default_opcode,
                            width: *width,
                            w: false,
                        },
                        dst_reg,
                        src1_reg,
                        src2_reg,
                    );
                } else {
                    let prefix = self.sse_prefix(op.x86_hint);
                    let opcode = self.sse_opcode(op.x86_hint, default_opcode);
                    if dst_reg != src1_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, 0x28, dst_reg, src1_reg);
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rr(prefix, opcode, dst_reg, src2_reg);
                }
            }

            OpKind::VShift {
                dst,
                src,
                amount,
                shift,
                elem,
                lanes,
            } => {
                if *shift != ShiftOp::Lsl || *elem != VecElementType::I32 {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("VShift {:?} {:?}x{}", shift, elem, lanes),
                    });
                }
                let imm = match amount {
                    SrcOperand::Imm(val) => {
                        if *val < 0 || *val > u8::MAX as i64 {
                            return Err(LowerError::InvalidOperand {
                                op: "VShift".to_string(),
                                operand: "imm out of range".to_string(),
                            });
                        }
                        *val as u8
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: "VShift with non-imm".to_string(),
                        });
                    }
                };

                let width = self.vec_width_from_lanes(*elem, *lanes).ok_or_else(|| {
                    LowerError::UnsupportedOp {
                        op: format!("VShift {:?}x{}", elem, lanes),
                    }
                })?;
                let dst_reg = self.get_dst_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                if !dst_reg.is_vec() || !src_reg.is_vec() {
                    return Err(LowerError::InvalidOperand {
                        op: "VShift".to_string(),
                        operand: "requires vector registers".to_string(),
                    });
                }

                if let Some(enc_hint) = self.vec_hint(op.x86_hint) {
                    let enc = self.coerce_vec_encoding(
                        VecEncoding { width, ..enc_hint },
                        &[dst_reg, src_reg],
                    );
                    self.emit_vec_shift_imm(enc, dst_reg, src_reg, imm);
                } else if width != VecWidth::V128 || self.vec_requires_vex(&[dst_reg, src_reg]) {
                    let kind = if self.vec_requires_evex(width, &[dst_reg, src_reg]) {
                        VecEncodingKind::Evex
                    } else {
                        VecEncodingKind::Vex
                    };
                    let enc = VecEncoding {
                        kind,
                        map: X86VecMap::Map0F,
                        pp: X86SsePrefix::OpSize,
                        opcode: 0x72,
                        width,
                        w: false,
                    };
                    self.emit_vec_shift_imm(enc, dst_reg, src_reg, imm);
                } else {
                    let prefix = Some(0x66);
                    if dst_reg != src_reg {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_sse_mov_rr(prefix, 0x6F, dst_reg, src_reg);
                    }
                    let mut emitter = X86Emitter::new(&mut self.code);
                    if let Some(prefix) = prefix {
                        emitter.code.emit_u8(prefix);
                    }
                    emitter.emit_rex_for_xmm(dst_reg, dst_reg);
                    emitter.code.emit_u8(0x0F);
                    emitter.code.emit_u8(0x72);
                    emitter.emit_modrm_digit(0b11, 6, dst_reg);
                    emitter.code.emit_u8(imm);
                }
            }

            OpKind::X86CheckAlignment { addr, alignment } => {
                self.emit_x86_check_alignment(op.guest_pc, addr, *alignment)?;
            }

            OpKind::X86CheckAlignmentAc { .. } => {
                self.emit_x86_check_alignment_ac(op)?;
            }

            OpKind::X86CacheControl { kind, .. } if *kind == X86CacheControlKind::Cldemote => {
                // CLDEMOTE is an architecturally ignorable cache-placement
                // hint and raises no memory-address exception. Executing no
                // host instruction therefore preserves guest semantics without
                // exposing the guest linear address to the host cache hierarchy.
            }

            OpKind::Load {
                dst,
                addr,
                width,
                sign,
            } => {
                // JIT memory mode: route through the MMU helper-call path
                // (translate + fault-bail) instead of a direct host-pointer load.
                if self.mem_helpers {
                    return self.emit_jit_mem_op(
                        op.guest_pc,
                        true,
                        Some(*dst),
                        None,
                        None,
                        None,
                        None,
                        addr,
                        *width,
                        *sign,
                        0,
                    );
                }
                let dst_reg = self.get_dst_reg(*dst)?;
                let op_width = width.to_op_width().unwrap_or(OpWidth::W64);
                let preserve_x86_partial = matches!(dst, VReg::Arch(ArchReg::X86(_)))
                    && matches!(op_width, OpWidth::W8 | OpWidth::W16)
                    && matches!(sign, SignExtend::Zero);
                let needs_extend = op_width != OpWidth::W64 && !preserve_x86_partial;

                match addr {
                    Address::Direct(base) => {
                        let base_reg = self.get_reg(*base)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rm(dst_reg, base_reg, 0, op_width);

                        // Sign/zero extend if loading smaller than 64-bit
                        if needs_extend {
                            match sign {
                                SignExtend::Zero => {
                                    // 32-bit loads automatically zero-extend
                                    if op_width != OpWidth::W32 {
                                        emitter.emit_movzx(
                                            dst_reg,
                                            dst_reg,
                                            op_width,
                                            OpWidth::W64,
                                        );
                                    }
                                }
                                SignExtend::Sign => {
                                    emitter.emit_movsx(dst_reg, dst_reg, op_width, OpWidth::W64);
                                }
                            }
                        }
                    }
                    Address::BaseOffset {
                        base,
                        offset,
                        disp_size,
                    } => {
                        let base_reg = self.get_reg(*base)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rm_disp(
                            dst_reg,
                            base_reg,
                            *offset as i32,
                            *disp_size,
                            op_width,
                        );

                        if needs_extend {
                            match sign {
                                SignExtend::Zero => {
                                    if op_width != OpWidth::W32 {
                                        emitter.emit_movzx(
                                            dst_reg,
                                            dst_reg,
                                            op_width,
                                            OpWidth::W64,
                                        );
                                    }
                                }
                                SignExtend::Sign => {
                                    emitter.emit_movsx(dst_reg, dst_reg, op_width, OpWidth::W64);
                                }
                            }
                        }
                    }
                    Address::PcRel { offset, base, .. } => {
                        let disp_offset = {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mov_rm_pcrel(dst_reg, 0, op_width)
                        };
                        let insn_end = self.code.position();

                        let disp = if let Some(base_pc) = base {
                            let target = (*base_pc as i64 + *offset) as u64;
                            let disp = if self.pcrel_adjust {
                                let next_rip = self.guest_base as i64 + insn_end as i64;
                                target as i64 - next_rip
                            } else {
                                *offset
                            };
                            if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                                return Err(LowerError::InvalidOperand {
                                    op: "Load".to_string(),
                                    operand: "PcRel offset out of range".to_string(),
                                });
                            }
                            self.relocations.push(Relocation {
                                offset: disp_offset,
                                kind: RelocKind::PcRel32,
                                target: RelocTarget::GuestAddr(target),
                            });
                            disp
                        } else {
                            let disp = *offset;
                            if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                                return Err(LowerError::InvalidOperand {
                                    op: "Load".to_string(),
                                    operand: "PcRel offset out of range".to_string(),
                                });
                            }
                            disp
                        };

                        self.code.patch_i32(disp_offset, disp as i32);

                        if needs_extend {
                            match sign {
                                SignExtend::Zero => {
                                    if op_width != OpWidth::W32 {
                                        let mut emitter = X86Emitter::new(&mut self.code);
                                        emitter.emit_movzx(
                                            dst_reg,
                                            dst_reg,
                                            op_width,
                                            OpWidth::W64,
                                        );
                                    }
                                }
                                SignExtend::Sign => {
                                    let mut emitter = X86Emitter::new(&mut self.code);
                                    emitter.emit_movsx(dst_reg, dst_reg, op_width, OpWidth::W64);
                                }
                            }
                        }
                    }
                    Address::Absolute(abs_addr) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rm_abs(dst_reg, *abs_addr, op_width);

                        if needs_extend {
                            match sign {
                                SignExtend::Zero => {
                                    if op_width != OpWidth::W32 {
                                        emitter.emit_movzx(
                                            dst_reg,
                                            dst_reg,
                                            op_width,
                                            OpWidth::W64,
                                        );
                                    }
                                }
                                SignExtend::Sign => {
                                    emitter.emit_movsx(dst_reg, dst_reg, op_width, OpWidth::W64);
                                }
                            }
                        }
                    }
                    Address::BaseIndexScale {
                        base,
                        index,
                        scale,
                        disp,
                        disp_size,
                    } => {
                        let base_reg = base.map(|b| self.get_reg(b)).transpose()?;
                        let index_reg = self.get_reg(*index)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_rm_sib_disp(
                            dst_reg, base_reg, index_reg, *scale, *disp, *disp_size, op_width,
                        );

                        if needs_extend {
                            match sign {
                                SignExtend::Zero => {
                                    if op_width != OpWidth::W32 {
                                        emitter.emit_movzx(
                                            dst_reg,
                                            dst_reg,
                                            op_width,
                                            OpWidth::W64,
                                        );
                                    }
                                }
                                SignExtend::Sign => {
                                    emitter.emit_movsx(dst_reg, dst_reg, op_width, OpWidth::W64);
                                }
                            }
                        }
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("Load with unsupported addressing: {:?}", addr),
                        });
                    }
                }
            }

            OpKind::Store { src, addr, width } => {
                // JIT memory mode: route through the MMU helper-call path.
                if self.mem_helpers {
                    let (src_reg, src_imm) = match src {
                        VReg::Imm(imm) => (None, Some(*imm)),
                        other => (Some(*other), None),
                    };
                    return self.emit_jit_mem_op(
                        op.guest_pc,
                        false,
                        None,
                        None,
                        src_reg,
                        src_imm,
                        None,
                        addr,
                        *width,
                        SignExtend::Zero,
                        0,
                    );
                }
                let op_width = width.to_op_width().unwrap_or(OpWidth::W64);

                if let VReg::Imm(imm) = src {
                    let imm_ok = match op_width {
                        OpWidth::W64 => *imm >= i32::MIN as i64 && *imm <= i32::MAX as i64,
                        OpWidth::W128 => false,
                        _ => true,
                    };

                    if imm_ok {
                        match addr {
                            Address::Direct(base) => {
                                let base_reg = self.get_reg(*base)?;
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_mi_disp(
                                    base_reg,
                                    0,
                                    DispSize::Auto,
                                    *imm,
                                    op_width,
                                );
                                return Ok(());
                            }
                            Address::BaseOffset {
                                base,
                                offset,
                                disp_size,
                            } => {
                                let base_reg = self.get_reg(*base)?;
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_mi_disp(
                                    base_reg,
                                    *offset as i32,
                                    *disp_size,
                                    *imm,
                                    op_width,
                                );
                                return Ok(());
                            }
                            Address::PcRel { offset, base, .. } => {
                                let disp_offset = {
                                    let mut emitter = X86Emitter::new(&mut self.code);
                                    emitter.emit_mov_mi_pcrel(0, op_width, *imm)
                                };
                                let insn_end = self.code.position();

                                let disp = if let Some(base_pc) = base {
                                    let target = (*base_pc as i64 + *offset) as u64;
                                    let disp = if self.pcrel_adjust {
                                        let next_rip = self.guest_base as i64 + insn_end as i64;
                                        target as i64 - next_rip
                                    } else {
                                        *offset
                                    };
                                    if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                                        return Err(LowerError::InvalidOperand {
                                            op: "Store".to_string(),
                                            operand: "PcRel offset out of range".to_string(),
                                        });
                                    }
                                    self.relocations.push(Relocation {
                                        offset: disp_offset,
                                        kind: RelocKind::PcRel32,
                                        target: RelocTarget::GuestAddr(target),
                                    });
                                    disp
                                } else {
                                    let disp = *offset;
                                    if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                                        return Err(LowerError::InvalidOperand {
                                            op: "Store".to_string(),
                                            operand: "PcRel offset out of range".to_string(),
                                        });
                                    }
                                    disp
                                };

                                self.code.patch_i32(disp_offset, disp as i32);
                                return Ok(());
                            }
                            Address::Absolute(abs_addr) => {
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_mi_abs(*abs_addr, *imm, op_width);
                                return Ok(());
                            }
                            Address::BaseIndexScale {
                                base,
                                index,
                                scale,
                                disp,
                                disp_size,
                            } => {
                                let base_reg = base.map(|b| self.get_reg(b)).transpose()?;
                                let index_reg = self.get_reg(*index)?;
                                let mut emitter = X86Emitter::new(&mut self.code);
                                emitter.emit_mov_mi_sib_disp(
                                    base_reg, index_reg, *scale, *disp, *disp_size, *imm, op_width,
                                );
                                return Ok(());
                            }
                            _ => {}
                        }
                    }
                }

                let src_reg = self.get_reg(*src)?;

                match addr {
                    Address::Direct(base) => {
                        let base_reg = self.get_reg(*base)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_mr(base_reg, 0, src_reg, op_width);
                    }
                    Address::BaseOffset {
                        base,
                        offset,
                        disp_size,
                    } => {
                        let base_reg = self.get_reg(*base)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_mr_disp(
                            base_reg,
                            *offset as i32,
                            *disp_size,
                            src_reg,
                            op_width,
                        );
                    }
                    Address::PcRel { offset, base, .. } => {
                        let disp_offset = {
                            let mut emitter = X86Emitter::new(&mut self.code);
                            emitter.emit_mov_mr_pcrel(0, src_reg, op_width)
                        };
                        let insn_end = self.code.position();

                        let disp = if let Some(base_pc) = base {
                            let target = (*base_pc as i64 + *offset) as u64;
                            let disp = if self.pcrel_adjust {
                                let next_rip = self.guest_base as i64 + insn_end as i64;
                                target as i64 - next_rip
                            } else {
                                *offset
                            };
                            if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                                return Err(LowerError::InvalidOperand {
                                    op: "Store".to_string(),
                                    operand: "PcRel offset out of range".to_string(),
                                });
                            }
                            self.relocations.push(Relocation {
                                offset: disp_offset,
                                kind: RelocKind::PcRel32,
                                target: RelocTarget::GuestAddr(target),
                            });
                            disp
                        } else {
                            let disp = *offset;
                            if disp < i32::MIN as i64 || disp > i32::MAX as i64 {
                                return Err(LowerError::InvalidOperand {
                                    op: "Store".to_string(),
                                    operand: "PcRel offset out of range".to_string(),
                                });
                            }
                            disp
                        };

                        self.code.patch_i32(disp_offset, disp as i32);
                    }
                    Address::Absolute(abs_addr) => {
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_mr_abs(*abs_addr, src_reg, op_width);
                    }
                    Address::BaseIndexScale {
                        base,
                        index,
                        scale,
                        disp,
                        disp_size,
                    } => {
                        let base_reg = base.map(|b| self.get_reg(b)).transpose()?;
                        let index_reg = self.get_reg(*index)?;
                        let mut emitter = X86Emitter::new(&mut self.code);
                        emitter.emit_mov_mr_sib_disp(
                            base_reg, index_reg, *scale, *disp, *disp_size, src_reg, op_width,
                        );
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("Store with unsupported addressing: {:?}", addr),
                        });
                    }
                }
            }

            OpKind::PredLoad {
                dst,
                cond,
                addr,
                width,
                signed,
            } => {
                let skip = self.emit_predicated_memory_guard("PredLoad", *cond, addr, None)?;
                let load = SmirOp::new(
                    op.id,
                    op.guest_pc,
                    OpKind::Load {
                        dst: *dst,
                        addr: addr.clone(),
                        width: *width,
                        sign: *signed,
                    },
                );
                self.lower_op(&load)?;
                self.patch_rel32_to_current(skip)?;
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::PredStore {
                src,
                cond,
                addr,
                width,
            } => {
                let skip =
                    self.emit_predicated_memory_guard("PredStore", *cond, addr, Some(src))?;
                let store = SmirOp::new(
                    op.id,
                    op.guest_pc,
                    OpKind::Store {
                        src: Self::pred_store_src_to_vreg(src)?,
                        addr: addr.clone(),
                        width: *width,
                    },
                );
                self.lower_op(&store)?;
                self.patch_rel32_to_current(skip)?;
                self.code.emit_u8(0x9D); // popfq
            }

            OpKind::RepStos {
                dst,
                src,
                count,
                width,
            } => {
                let dst_reg = self.get_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let count_reg = self.get_reg(*count)?;

                if dst_reg != PhysReg::Rdi || src_reg != PhysReg::Rax || count_reg != PhysReg::Rcx {
                    return Err(LowerError::InvalidOperand {
                        op: "RepStos".to_string(),
                        operand: "requires RDI/RAX/RCX".to_string(),
                    });
                }

                let mut emitter = X86Emitter::new(&mut self.code);
                match width {
                    MemWidth::B1 | MemWidth::B2 | MemWidth::B4 | MemWidth::B8 => {
                        emitter.emit_rep_stos(*width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("RepStos width {:?}", width),
                        });
                    }
                }
            }

            OpKind::RepMovs {
                dst,
                src,
                count,
                width,
            } => {
                let dst_reg = self.get_reg(*dst)?;
                let src_reg = self.get_reg(*src)?;
                let count_reg = self.get_reg(*count)?;

                if dst_reg != PhysReg::Rdi || src_reg != PhysReg::Rsi || count_reg != PhysReg::Rcx {
                    return Err(LowerError::InvalidOperand {
                        op: "RepMovs".to_string(),
                        operand: "requires RDI/RSI/RCX".to_string(),
                    });
                }

                let mut emitter = X86Emitter::new(&mut self.code);
                match width {
                    MemWidth::B1 | MemWidth::B2 | MemWidth::B4 | MemWidth::B8 => {
                        emitter.emit_rep_movs(*width);
                    }
                    _ => {
                        return Err(LowerError::UnsupportedOp {
                            op: format!("RepMovs width {:?}", width),
                        });
                    }
                }
            }

            OpKind::X86String {
                kind,
                rep,
                accumulator,
                src_index,
                dst_index,
                count,
                src_segment,
                width,
                address_width,
            } => {
                let require = |actual: PhysReg, expected: PhysReg, role: &str| {
                    if actual == expected {
                        Ok(())
                    } else {
                        Err(LowerError::InvalidOperand {
                            op: format!("X86String {kind:?} {rep:?}"),
                            operand: format!("{role} requires {expected:?}"),
                        })
                    }
                };
                if matches!(
                    kind,
                    X86StringKind::Stos | X86StringKind::Lods | X86StringKind::Scas
                ) {
                    require(self.get_reg(*accumulator)?, PhysReg::Rax, "accumulator")?;
                }
                if matches!(
                    kind,
                    X86StringKind::Movs | X86StringKind::Lods | X86StringKind::Cmps
                ) {
                    require(self.get_reg(*src_index)?, PhysReg::Rsi, "source index")?;
                }
                if matches!(
                    kind,
                    X86StringKind::Movs
                        | X86StringKind::Stos
                        | X86StringKind::Scas
                        | X86StringKind::Cmps
                ) {
                    require(self.get_reg(*dst_index)?, PhysReg::Rdi, "destination index")?;
                }
                if *rep != X86RepMode::None {
                    require(self.get_reg(*count)?, PhysReg::Rcx, "repeat count")?;
                }
                if src_segment.is_some() {
                    return Err(LowerError::UnsupportedOp {
                        op: format!("X86String {kind:?} with guest segment base"),
                    });
                }

                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_x86_string(*kind, *rep, *width, *address_width)?;
            }

            OpKind::X86ReadTsc(..) => self.emit_x86_read_tsc(op)?,

            OpKind::X86ReadPmc(..) => self.emit_x86_read_pmc(op)?,

            OpKind::X86ReadPid { dst } => self.lower_x86_read_pid(*dst)?,

            OpKind::X86XGetBv {
                dst_low,
                dst_high,
                selector,
            } => {
                let low = self.get_dst_reg(*dst_low)?;
                let high = self.get_dst_reg(*dst_high)?;
                let selector = self.get_reg(*selector)?;
                if low != PhysReg::Rax || high != PhysReg::Rdx || selector != PhysReg::Rcx {
                    return Err(LowerError::InvalidOperand {
                        op: "X86XGetBv".to_string(),
                        operand: "requires EAX/EDX destinations and ECX selector".to_string(),
                    });
                }

                // Preserve all architectural flags and the old RAX until both
                // fault conditions have been ruled out. A deoptimization must
                // restart XGETBV in the interpreter with byte-exact input state.
                self.code.emit_u8(0x9C); // pushfq
                self.code.emit_u8(0x50); // push rax
                self.code.emit_u8(0x48);
                self.code.emit_u8(0x8B);
                self.code.emit_u8(0x45);
                self.code.emit_u8(X86_STATE_PTR_AT_RBP as u8); // mov rax,[rbp+state]

                // test dword [rax+cr4], CR4.OSXSAVE
                self.code.emit_u8(0xF7);
                self.code.emit_u8(0x80);
                self.code.emit_u32(X86_GUEST_CR4_OFFSET as u32);
                self.code.emit_u32(1 << 18);
                // jz .fault (#UD in the interpreter)
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0x84);
                let osxsave_fault = self.code.position();
                self.code.emit_u32(0);

                // Only XCR0 (ECX=0) and XINUSE (ECX=1) exist in this model.
                self.code.emit_u8(0x83);
                self.code.emit_u8(0xF9);
                self.code.emit_u8(0x01); // cmp ecx,1
                // ja .fault (#GP(0) in the interpreter)
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0x87);
                let selector_fault = self.code.position();
                self.code.emit_u32(0);

                // rdx = XCR0; ECX=1 selects XINUSE & XCR0.
                self.code.emit_u8(0x48);
                self.code.emit_u8(0x8B);
                self.code.emit_u8(0x90);
                self.code.emit_u32(X86_GUEST_XCR0_OFFSET as u32);
                self.code.emit_u8(0x85);
                self.code.emit_u8(0xC9); // test ecx,ecx
                self.code.emit_u8(0x0F);
                self.code.emit_u8(0x84); // jz .selected
                let xcr0_selected = self.code.position();
                self.code.emit_u32(0);
                self.code.emit_u8(0x48);
                self.code.emit_u8(0x23);
                self.code.emit_u8(0x90);
                self.code.emit_u32(X86_GUEST_XGETBV1_OFFSET as u32); // and rdx,[rax+xgetbv1]
                let selected = self.code.position();
                self.code.patch_i32(
                    xcr0_selected,
                    (selected as i64 - (xcr0_selected as i64 + 4)) as i32,
                );

                // Split the selected 64-bit value into zero-extended EDX:EAX.
                self.code.emit_u8(0x48);
                self.code.emit_u8(0x89);
                self.code.emit_u8(0xD0); // mov rax,rdx
                self.code.emit_u8(0x48);
                self.code.emit_u8(0xC1);
                self.code.emit_u8(0xEA);
                self.code.emit_u8(0x20); // shr rdx,32
                self.code.emit_u8(0x89);
                self.code.emit_u8(0xC0); // mov eax,eax (zero-extend low half)
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 8); // discard saved RAX
                }
                self.code.emit_u8(0x9D); // popfq
                self.code.emit_u8(0xE9); // jmp .done
                let success_done = self.code.position();
                self.code.emit_u32(0);

                let fault = self.code.position();
                for branch in [osxsave_fault, selector_fault] {
                    self.code
                        .patch_i32(branch, (fault as i64 - (branch as i64 + 4)) as i32);
                }
                self.code.emit_u8(0x58); // restore old RAX
                self.code.emit_u8(0x9D); // restore flags
                self.emit_native_exit(op.guest_pc);

                let done = self.code.position();
                self.code.patch_i32(
                    success_done,
                    (done as i64 - (success_done as i64 + 4)) as i32,
                );
            }

            OpKind::IoIn { .. } | OpKind::IoOut { .. } => {
                return Err(LowerError::UnsupportedOp {
                    op: "scalar port I/O requires terminal helper-backed lowering".to_string(),
                });
            }

            OpKind::X86Enter(..) => {
                return Err(LowerError::UnsupportedOp {
                    op: "ENTER requires exact helper-backed block lowering".to_string(),
                });
            }

            _ => return self.lower_op_extensions(op),
        }

        Ok(())
    }
}
