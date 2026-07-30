//! AMD XOP packed rotate and signed-direction shift lifting.

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86VecAlign, X86XopPackedBitKind};
use crate::smir::ir::types::{OpId, SrcOperand, VecCmpCond, VecElementType, VecWidth};
use crate::smir::lift::x86_64::{X86_64Lifter, X86Prefix, build_rex, decode_modrm};
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

#[inline]
fn memory_uses_stack_segment(modrm: u8, following: &[u8], segment_override: Option<u8>) -> bool {
    match segment_override {
        Some(0x36) => return true,
        Some(_) => return false,
        None => {}
    }

    let mod_bits = modrm >> 6;
    let rm = modrm & 7;
    if rm == 4 {
        let Some(&sib) = following.first() else {
            return false;
        };
        let base = sib & 7;
        !(mod_bits == 0 && base == 5) && matches!(base, 4 | 5)
    } else {
        !(mod_bits == 0 && rm == 5) && matches!(rm, 4 | 5)
    }
}

impl X86_64Lifter {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lift_xop_vpcom(
        &self,
        bytes: &[u8],
        legacy: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
        lead: usize,
        p0: u8,
        opcode: u8,
        w: bool,
        vvvv: u8,
        l: u8,
        pp: u8,
    ) -> Result<LiftResult, LiftError> {
        let opcode_end = lead + 4;
        if w || l != 0 || pp != 0 {
            return Ok(Self::xop_invalid(opcode_end));
        }

        let (elem, signed) = match opcode {
            0xCC => (VecElementType::I8, true),
            0xCD => (VecElementType::I16, true),
            0xCE => (VecElementType::I32, true),
            0xCF => (VecElementType::I64, true),
            0xEC => (VecElementType::I8, false),
            0xED => (VecElementType::I16, false),
            0xEE => (VecElementType::I32, false),
            0xEF => (VecElementType::I64, false),
            _ => unreachable!("VPCOM dispatch validated opcode"),
        };
        let r = ((p0 >> 7) & 1) ^ 1;
        let x = ((p0 >> 6) & 1) ^ 1;
        let b = ((p0 >> 5) & 1) ^ 1;
        let modrm_prefix = X86Prefix {
            rex: build_rex(r, x, b, false),
            address_size_override: legacy.address_size_override,
            segment_override: legacy.segment_override,
            cursor: opcode_end,
            ..X86Prefix::default()
        };
        let modrm =
            decode_modrm(&bytes[opcode_end..], &modrm_prefix, pc).map_err(|error| match error {
                LiftError::Incomplete { addr, have, need } => LiftError::Incomplete {
                    addr,
                    have: opcode_end + have,
                    need: opcode_end + need,
                },
                error => error,
            })?;
        let bytes_consumed = opcode_end + modrm.bytes_consumed + 1;
        if bytes.len() < bytes_consumed {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: bytes_consumed,
            });
        }
        let predicate = bytes[bytes_consumed - 1] & 7;
        let cond = match (predicate, signed) {
            (0, true) => VecCmpCond::Lt,
            (1, true) => VecCmpCond::Le,
            (2, true) => VecCmpCond::Gt,
            (3, true) => VecCmpCond::Ge,
            (0, false) => VecCmpCond::Ltu,
            (1, false) => VecCmpCond::Leu,
            (2, false) => VecCmpCond::Gtu,
            (3, false) => VecCmpCond::Geu,
            (4, _) => VecCmpCond::Eq,
            (5, _) => VecCmpCond::Ne,
            (6, _) => VecCmpCond::False,
            (7, _) => VecCmpCond::True,
            _ => unreachable!("VPCOM predicate is masked to three bits"),
        };
        let next_pc = pc.wrapping_add(bytes_consumed as u64);
        let mut ops = vec![SmirOp::new(OpId(0), pc, OpKind::X86RequireXop)];
        let source2 = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(
                modrm.addr.as_ref().expect("decoded VPCOM memory address"),
                next_pc,
                ctx,
            );
            ops.extend(pre_ops);
            let stack_segment = memory_uses_stack_segment(
                modrm.byte,
                &bytes[opcode_end + 1..],
                legacy.segment_override,
            );
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86CheckAlignmentAc {
                    addr: addr.clone(),
                    access_size: 16,
                    alignment: 16,
                    stack_segment,
                    natural_alignment: false,
                },
            ));
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Aligned),
            ));
            loaded
        } else {
            self.xmm(modrm.rm)
        };
        ops.push(SmirOp::with_hint(
            OpId(ops.len() as u16),
            pc,
            OpKind::VCmp {
                dst: self.xmm(modrm.reg),
                src1: self.xmm(vvvv),
                src2: source2,
                cond,
                elem,
                lanes: VecWidth::V128.lanes(elem) as u8,
            },
            X86OpHint::XopVpcom,
        ));
        Ok(LiftResult::fallthrough(ops, bytes_consumed))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lift_xop_vpcmov(
        &self,
        bytes: &[u8],
        legacy: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
        lead: usize,
        p0: u8,
        w: bool,
        vvvv: u8,
        l: u8,
        pp: u8,
    ) -> Result<LiftResult, LiftError> {
        let opcode_end = lead + 4;
        if pp != 0 {
            return Ok(Self::xop_invalid(opcode_end));
        }

        let r = ((p0 >> 7) & 1) ^ 1;
        let x = ((p0 >> 6) & 1) ^ 1;
        let b = ((p0 >> 5) & 1) ^ 1;
        let modrm_prefix = X86Prefix {
            rex: build_rex(r, x, b, w),
            address_size_override: legacy.address_size_override,
            segment_override: legacy.segment_override,
            cursor: opcode_end,
            ..X86Prefix::default()
        };
        let modrm =
            decode_modrm(&bytes[opcode_end..], &modrm_prefix, pc).map_err(|error| match error {
                LiftError::Incomplete { addr, have, need } => LiftError::Incomplete {
                    addr,
                    have: opcode_end + have,
                    need: opcode_end + need,
                },
                error => error,
            })?;
        let bytes_consumed = opcode_end + modrm.bytes_consumed + 1;
        if bytes.len() < bytes_consumed {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: bytes_consumed,
            });
        }

        let next_pc = pc.wrapping_add(bytes_consumed as u64);
        let width = if l == 0 {
            VecWidth::V128
        } else {
            VecWidth::V256
        };
        // The dynamic #UD/#NM guard must precede address generation, alignment
        // validation, and the memory read.
        let mut ops = vec![SmirOp::new(OpId(0), pc, OpKind::X86RequireXop)];
        let rm_operand = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(
                modrm.addr.as_ref().expect("decoded VPCMOV memory address"),
                next_pc,
                ctx,
            );
            ops.extend(pre_ops);
            let stack_segment = memory_uses_stack_segment(
                modrm.byte,
                &bytes[opcode_end + 1..],
                legacy.segment_override,
            );
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86CheckAlignmentAc {
                    addr: addr.clone(),
                    access_size: width.bytes() as u8,
                    alignment: 16,
                    stack_segment,
                    natural_alignment: false,
                },
            ));
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width,
                },
                X86OpHint::VecAlign(X86VecAlign::Aligned),
            ));
            loaded
        } else {
            self.vec_reg(modrm.rm, width)
        };

        let selected = self.vec_reg(bytes[bytes_consumed - 1] >> 4, width);
        let (src_false, mask) = if w {
            (selected, rm_operand)
        } else {
            (rm_operand, selected)
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VBitSelect {
                dst: self.vec_reg(modrm.reg, width),
                mask,
                src_true: self.vec_reg(vvvv, width),
                src_false,
                width,
            },
        ));
        Ok(LiftResult::fallthrough(ops, bytes_consumed))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn lift_xop_packed_bit(
        &self,
        bytes: &[u8],
        legacy: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
        lead: usize,
        p0: u8,
        opcode: u8,
        map: u8,
        w: bool,
        vvvv: u8,
        l: u8,
        pp: u8,
    ) -> Result<LiftResult, LiftError> {
        let immediate = map == 8 && matches!(opcode, 0xC0..=0xC3);
        let variable = map == 9 && matches!(opcode, 0x90..=0x9B);
        debug_assert!(immediate || variable);
        let opcode_end = lead + 4;
        if l != 0 || pp != 0 || immediate && (w || vvvv != 0) {
            return Ok(Self::xop_invalid(opcode_end));
        }

        let r = ((p0 >> 7) & 1) ^ 1;
        let x = ((p0 >> 6) & 1) ^ 1;
        let b = ((p0 >> 5) & 1) ^ 1;
        let modrm_prefix = X86Prefix {
            rex: build_rex(r, x, b, w),
            address_size_override: legacy.address_size_override,
            segment_override: legacy.segment_override,
            cursor: opcode_end,
            ..X86Prefix::default()
        };
        let modrm =
            decode_modrm(&bytes[opcode_end..], &modrm_prefix, pc).map_err(|error| match error {
                LiftError::Incomplete { addr, have, need } => LiftError::Incomplete {
                    addr,
                    have: opcode_end + have,
                    need: opcode_end + need,
                },
                error => error,
            })?;
        let immediate_bytes = usize::from(immediate);
        let bytes_consumed = opcode_end + modrm.bytes_consumed + immediate_bytes;
        if bytes.len() < bytes_consumed {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: bytes_consumed,
            });
        }
        let next_pc = pc.wrapping_add(bytes_consumed as u64);
        let (kind, elem) = match opcode {
            0x90 | 0xC0 => (X86XopPackedBitKind::Rotate, VecElementType::I8),
            0x91 | 0xC1 => (X86XopPackedBitKind::Rotate, VecElementType::I16),
            0x92 | 0xC2 => (X86XopPackedBitKind::Rotate, VecElementType::I32),
            0x93 | 0xC3 => (X86XopPackedBitKind::Rotate, VecElementType::I64),
            0x94 => (X86XopPackedBitKind::LogicalShift, VecElementType::I8),
            0x95 => (X86XopPackedBitKind::LogicalShift, VecElementType::I16),
            0x96 => (X86XopPackedBitKind::LogicalShift, VecElementType::I32),
            0x97 => (X86XopPackedBitKind::LogicalShift, VecElementType::I64),
            0x98 => (X86XopPackedBitKind::ArithmeticShift, VecElementType::I8),
            0x99 => (X86XopPackedBitKind::ArithmeticShift, VecElementType::I16),
            0x9A => (X86XopPackedBitKind::ArithmeticShift, VecElementType::I32),
            0x9B => (X86XopPackedBitKind::ArithmeticShift, VecElementType::I64),
            _ => unreachable!("XOP packed-bit dispatch validated opcode"),
        };

        // Every dynamic #UD/#NM condition precedes address generation and all
        // memory validation. The guard deoptimizes to the direct path so it can
        // distinguish the exact exception without committing later operations.
        let mut ops = vec![SmirOp::new(OpId(0), pc, OpKind::X86RequireXop)];
        let rm_operand = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(
                modrm.addr.as_ref().expect("decoded XOP memory address"),
                next_pc,
                ctx,
            );
            ops.extend(pre_ops);
            let stack_segment = memory_uses_stack_segment(
                modrm.byte,
                &bytes[opcode_end + 1..],
                legacy.segment_override,
            );
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::X86CheckAlignmentAc {
                    addr: addr.clone(),
                    access_size: 16,
                    alignment: 16,
                    stack_segment,
                    natural_alignment: false,
                },
            ));
            let loaded = ctx.alloc_vreg();
            ops.push(SmirOp::with_hint(
                OpId(ops.len() as u16),
                pc,
                OpKind::VLoad {
                    dst: loaded,
                    addr,
                    width: VecWidth::V128,
                },
                X86OpHint::VecAlign(X86VecAlign::Aligned),
            ));
            loaded
        } else {
            self.xmm(modrm.rm)
        };

        let (src, count) = if immediate {
            let imm = bytes[opcode_end + modrm.bytes_consumed];
            (rm_operand, SrcOperand::Imm(i64::from(imm)))
        } else if w {
            (self.xmm(vvvv), SrcOperand::Reg(rm_operand))
        } else {
            (rm_operand, SrcOperand::Reg(self.xmm(vvvv)))
        };
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86XopPackedBit {
                dst: self.xmm(modrm.reg),
                src,
                count,
                elem,
                kind,
            },
        ));
        Ok(LiftResult::fallthrough(ops, bytes_consumed))
    }
}
