//! State-backed x86 Group-1 bitwise/compare lowering.
//!
//! `ADD`/`SUB` naming guest RSP/RBP already have a state-backed lowering (see
//! [`X86_64Lowerer::lower_state_backed_stack_gpr_alu`]). The remaining Group-1
//! forms — `AND`, `OR`, `XOR`, `ADC`, `SBB`, `CMP` — plus `TEST` were still
//! unconditional interpreter frontiers, so common stack idioms such as
//! `and rsp,-16`, `test rsp,0Fh`, and `cmp rbp,rax` rejected the whole hot
//! region.
//!
//! The lowering reuses the same discipline: spill the legacy GPR file, compute
//! from the resulting coherent `GuestRegs` snapshot, then commit the
//! architectural destination (if any) back to its slot. `ADC`/`SBB` consume the
//! incoming carry: no instruction between the region entry and the arithmetic
//! writes flags, so the architectural CF is still live at that point.

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{ArchReg, OpWidth, SrcOperand, VReg};
use crate::smir::lower::LowerError;
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::x86_64::{X86_64Lowerer, X86Emitter};

/// Group-1 arithmetic/logic operation selector for the state-backed lowering.
/// `Cmp` and `Test` produce flags only and have no architectural destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86StateGroup1Op {
    Or,
    Adc,
    Sbb,
    And,
    Xor,
    Cmp,
    Test,
}

/// One validated state-backed Group-1 operation.
pub(crate) struct X86StateGroup1<'a> {
    pub(crate) op: X86StateGroup1Op,
    pub(crate) dst: Option<VReg>,
    pub(crate) src1: VReg,
    pub(crate) src2: &'a SrcOperand,
    pub(crate) width: OpWidth,
    pub(crate) flags: FlagUpdate,
}

fn gpr_index(reg: &VReg) -> Option<u8> {
    match reg {
        VReg::Arch(ArchReg::X86(x86)) => x86.gpr_index(),
        _ => None,
    }
}

fn is_stack(reg: &VReg) -> bool {
    gpr_index(reg).is_some_and(|index| matches!(index, 4 | 5))
}

/// Decode `kind` into the exact state-backed Group-1 shape, or `None` when it
/// is not one of the modeled operations, does not name guest RSP/RBP, or uses
/// an operand class the lowering does not reconstruct.
pub(crate) fn x86_state_backed_stack_group1(kind: &OpKind) -> Option<X86StateGroup1<'_>> {
    let (op, dst, src1, src2, width, flags) = match kind {
        OpKind::Or {
            dst,
            src1,
            src2,
            width,
            flags,
        } => (
            X86StateGroup1Op::Or,
            Some(*dst),
            *src1,
            src2,
            *width,
            *flags,
        ),
        OpKind::Adc {
            dst,
            src1,
            src2,
            width,
            flags,
        } => (
            X86StateGroup1Op::Adc,
            Some(*dst),
            *src1,
            src2,
            *width,
            *flags,
        ),
        OpKind::Sbb {
            dst,
            src1,
            src2,
            width,
            flags,
        } => (
            X86StateGroup1Op::Sbb,
            Some(*dst),
            *src1,
            src2,
            *width,
            *flags,
        ),
        OpKind::And {
            dst,
            src1,
            src2,
            width,
            flags,
        } => (
            X86StateGroup1Op::And,
            Some(*dst),
            *src1,
            src2,
            *width,
            *flags,
        ),
        OpKind::Xor {
            dst,
            src1,
            src2,
            width,
            flags,
        } => (
            X86StateGroup1Op::Xor,
            Some(*dst),
            *src1,
            src2,
            *width,
            *flags,
        ),
        OpKind::Cmp { src1, src2, width } => (
            X86StateGroup1Op::Cmp,
            None,
            *src1,
            src2,
            *width,
            FlagUpdate::All,
        ),
        OpKind::Test { src1, src2, width } => (
            X86StateGroup1Op::Test,
            None,
            *src1,
            src2,
            *width,
            FlagUpdate::All,
        ),
        _ => return None,
    };

    if !matches!(
        width,
        OpWidth::W8 | OpWidth::W16 | OpWidth::W32 | OpWidth::W64
    ) || !matches!(flags, FlagUpdate::None | FlagUpdate::All)
    {
        return None;
    }
    if dst.is_some_and(|dst| gpr_index(&dst).is_none()) || gpr_index(&src1).is_none() {
        return None;
    }
    match src2 {
        SrcOperand::Reg(src2) => {
            if gpr_index(src2).is_none() {
                return None;
            }
        }
        // Group 1 and TEST encode only a sign-extended imm32 at 64-bit operand
        // size, so a wider immediate cannot be represented.
        SrcOperand::Imm(value) => {
            if width == OpWidth::W64 && i32::try_from(*value).is_err() {
                return None;
            }
        }
        _ => return None,
    }
    if !(dst.is_some_and(|dst| is_stack(&dst))
        || is_stack(&src1)
        || matches!(src2, SrcOperand::Reg(src2) if is_stack(src2)))
    {
        return None;
    }

    Some(X86StateGroup1 {
        op,
        dst,
        src1,
        src2,
        width,
        flags,
    })
}

/// Whether `op` is a modeled Group-1 operation naming guest RSP/RBP but not an
/// admitted shape. Used by the native gate to fail closed rather than let the
/// ordinary lowering compute against the host stack pointer / frame pointer.
pub(crate) fn x86_state_backed_stack_group1_candidate(op: &SmirOp) -> bool {
    let (dst, src1, src2) = match &op.kind {
        OpKind::Or {
            dst, src1, src2, ..
        }
        | OpKind::Adc {
            dst, src1, src2, ..
        }
        | OpKind::Sbb {
            dst, src1, src2, ..
        }
        | OpKind::And {
            dst, src1, src2, ..
        }
        | OpKind::Xor {
            dst, src1, src2, ..
        } => (Some(*dst), *src1, src2),
        OpKind::Cmp { src1, src2, .. } | OpKind::Test { src1, src2, .. } => (None, *src1, src2),
        _ => return false,
    };
    dst.is_some_and(|dst| is_stack(&dst))
        || is_stack(&src1)
        || matches!(src2, SrcOperand::Reg(src2) if is_stack(src2))
}

/// Whether `op` is an admitted state-backed Group-1 shape. Encoding-direction
/// hints do not change the architectural result and are accepted; every other
/// hint class leaves the modeled shape.
pub(crate) fn x86_state_backed_stack_group1_valid(op: &SmirOp) -> bool {
    x86_state_backed_stack_group1_lowerable(op).is_some()
}

/// The admitted state-backed Group-1 shape for `op`, if any.
pub(crate) fn x86_state_backed_stack_group1_lowerable(op: &SmirOp) -> Option<X86StateGroup1<'_>> {
    if !matches!(op.x86_hint, None | Some(X86OpHint::AluEncoding(_))) {
        return None;
    }
    x86_state_backed_stack_group1(&op.kind)
}

impl X86_64Lowerer {
    pub(crate) fn lower_state_backed_stack_gpr_group1(
        &mut self,
        shape: &X86StateGroup1<'_>,
    ) -> Result<(), LowerError> {
        let invalid = |operand: &str| LowerError::InvalidOperand {
            op: "state-backed stack Group-1".to_string(),
            operand: operand.to_string(),
        };
        let width = shape.width;
        let dst_idx = match shape.dst {
            Some(dst) => Some(
                Self::x86_gpr_index(dst)
                    .ok_or_else(|| invalid("destination is not an architectural x86 GPR"))?,
            ),
            None => None,
        };
        let src1_idx = Self::x86_gpr_index(shape.src1)
            .ok_or_else(|| invalid("source is not an architectural x86 GPR"))?;
        let src2_idx = match shape.src2 {
            SrcOperand::Reg(src2) => Some(
                Self::x86_gpr_index(*src2)
                    .ok_or_else(|| invalid("source is not an architectural x86 GPR"))?,
            ),
            SrcOperand::Imm(value) => {
                if width == OpWidth::W64 && i32::try_from(*value).is_err() {
                    return Err(invalid("64-bit immediate is not a sign-extended imm32"));
                }
                None
            }
            _ => return Err(invalid("non-scalar source operand")),
        };

        self.code.emit_u8(0x50); // push guest RAX while creating the state snapshot
        self.emit_load_state_ptr_rax();
        // A flag-suppressed form (APX NF) must leave RFLAGS untouched; the
        // architectural forms publish the flags their arithmetic produces, and
        // every instruction after it in this sequence is MOV/LEA.
        let preserve_flags = !shape.flags.updates_any();
        if preserve_flags {
            self.code.emit_u8(0x9C); // pushfq; guest RAX is now at [rsp+8]
        }
        self.emit_spill_legacy_gprs_to_state_from_rax(if preserve_flags { 8 } else { 0 });

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(src1_idx) * 8, width);
            match (shape.src2, src2_idx) {
                (SrcOperand::Reg(_), Some(index)) => {
                    emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rax, i32::from(index) * 8, width);
                    match shape.op {
                        X86StateGroup1Op::Or => {
                            emitter.emit_or_rr(PhysReg::Rdx, PhysReg::Rdi, width)
                        }
                        X86StateGroup1Op::Adc => {
                            emitter.emit_adc_rr(PhysReg::Rdx, PhysReg::Rdi, width)
                        }
                        X86StateGroup1Op::Sbb => {
                            emitter.emit_sbb_rr(PhysReg::Rdx, PhysReg::Rdi, width)
                        }
                        X86StateGroup1Op::And => {
                            emitter.emit_and_rr(PhysReg::Rdx, PhysReg::Rdi, width)
                        }
                        X86StateGroup1Op::Xor => {
                            emitter.emit_xor_rr(PhysReg::Rdx, PhysReg::Rdi, width)
                        }
                        X86StateGroup1Op::Cmp => {
                            emitter.emit_cmp_rr(PhysReg::Rdx, PhysReg::Rdi, width)
                        }
                        X86StateGroup1Op::Test => {
                            emitter.emit_test_rr(PhysReg::Rdx, PhysReg::Rdi, width)
                        }
                    }
                }
                (SrcOperand::Imm(value), None) => match shape.op {
                    X86StateGroup1Op::Or => emitter.emit_or_ri(PhysReg::Rdx, *value, width),
                    X86StateGroup1Op::Adc => emitter.emit_adc_ri(PhysReg::Rdx, *value, width),
                    X86StateGroup1Op::Sbb => emitter.emit_sbb_ri(PhysReg::Rdx, *value, width),
                    X86StateGroup1Op::And => emitter.emit_and_ri(PhysReg::Rdx, *value, width),
                    X86StateGroup1Op::Xor => emitter.emit_xor_ri(PhysReg::Rdx, *value, width),
                    X86StateGroup1Op::Cmp => emitter.emit_cmp_ri(PhysReg::Rdx, *value, width),
                    X86StateGroup1Op::Test => emitter.emit_test_ri(PhysReg::Rdx, *value, width),
                },
                _ => return Err(invalid("non-scalar source operand")),
            }
        }

        if let Some(dst_idx) = dst_idx {
            self.emit_store_gpr_slot_from_reg(dst_idx, PhysReg::Rdx, width)?;
            if dst_idx == 5 {
                let commit_width = if matches!(width, OpWidth::W8 | OpWidth::W16) {
                    width
                } else {
                    OpWidth::W64
                };
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
            }
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        if preserve_flags {
            self.code.emit_u8(0x9D); // popfq
        }
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }
}
