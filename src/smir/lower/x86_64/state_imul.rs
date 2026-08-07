//! State-backed x86 signed-multiply lowering.
//!
//! Guest RSP/RBP and APX EGPRs do not participate in the native identity GPR
//! map. Register-only IMUL forms naming one of those registers therefore
//! snapshot the complete legacy GPR file, compute through scratch registers,
//! and commit the architectural result through `GuestRegs`.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum X86StateImulSource {
    Reg(u8),
    Imm { value: i32, use_imm8: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum X86StateImul {
    Implicit {
        source: u8,
        width: OpWidth,
        flags: FlagUpdate,
    },
    Truncated {
        dst: u8,
        src1: u8,
        src2: X86StateImulSource,
        width: OpWidth,
        flags: FlagUpdate,
    },
}

fn arch_gpr_index(reg: &VReg) -> Option<u8> {
    match reg {
        VReg::Arch(ArchReg::X86(x86)) => x86.gpr_index(),
        _ => None,
    }
}

fn source_names_state_gpr(source: &SrcOperand) -> bool {
    match source {
        SrcOperand::Reg(reg)
        | SrcOperand::Shifted { reg, .. }
        | SrcOperand::Extended { reg, .. } => x86_state_backed_arch_gpr(reg),
        SrcOperand::Imm(_) | SrcOperand::Imm64(_) => false,
    }
}

/// Whether `op` is a signed multiply naming a non-identity x86 GPR. This is
/// deliberately broader than the admitted shape so malformed state-backed
/// operations cannot fall through to the ordinary identity-map lowering.
pub(crate) fn x86_state_imul_candidate(op: &SmirOp) -> bool {
    matches!(
        &op.kind,
        OpKind::MulS {
            dst_lo,
            dst_hi,
            src1,
            src2,
            ..
        } if x86_state_backed_arch_gpr(dst_lo)
            || dst_hi.as_ref().is_some_and(x86_state_backed_arch_gpr)
            || x86_state_backed_arch_gpr(src1)
            || source_names_state_gpr(src2)
    )
}

fn decode_state_imul(op: &SmirOp) -> Option<X86StateImul> {
    if !x86_state_imul_candidate(op) {
        return None;
    }
    let OpKind::MulS {
        dst_lo,
        dst_hi,
        src1,
        src2,
        width,
        flags,
    } = &op.kind
    else {
        return None;
    };
    if !matches!(flags, FlagUpdate::None | FlagUpdate::All) {
        return None;
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    let implicit = *dst_lo == rax
        && *src1 == rax
        && match width {
            OpWidth::W8 => dst_hi.is_none(),
            OpWidth::W16 | OpWidth::W32 | OpWidth::W64 => *dst_hi == Some(rdx),
            OpWidth::W128 => false,
        };
    if implicit {
        let SrcOperand::Reg(source) = src2 else {
            return None;
        };
        return (op.x86_hint.is_none() && arch_gpr_index(source).is_some()).then(|| {
            X86StateImul::Implicit {
                source: arch_gpr_index(source).expect("checked architectural GPR"),
                width: *width,
                flags: *flags,
            }
        });
    }

    if dst_hi.is_some() || !matches!(width, OpWidth::W16 | OpWidth::W32 | OpWidth::W64) {
        return None;
    }
    let (Some(dst), Some(src1)) = (arch_gpr_index(dst_lo), arch_gpr_index(src1)) else {
        return None;
    };
    let src2 = match (src2, op.x86_hint) {
        (SrcOperand::Reg(source), None) => X86StateImulSource::Reg(arch_gpr_index(source)?),
        (SrcOperand::Imm(value), Some(X86OpHint::ImulImm8)) if i8::try_from(*value).is_ok() => {
            X86StateImulSource::Imm {
                value: *value as i32,
                use_imm8: true,
            }
        }
        (SrcOperand::Imm(value), Some(X86OpHint::ImulImm32))
            if match width {
                OpWidth::W16 => i16::try_from(*value).is_ok(),
                OpWidth::W32 | OpWidth::W64 => i32::try_from(*value).is_ok(),
                _ => false,
            } =>
        {
            X86StateImulSource::Imm {
                value: *value as i32,
                use_imm8: false,
            }
        }
        _ => return None,
    };
    Some(X86StateImul::Truncated {
        dst,
        src1,
        src2,
        width: *width,
        flags: *flags,
    })
}

/// Validate the exact implicit, two-operand, and three-operand IMUL shapes
/// emitted by the legacy, REX2, and APX EVEX lifters.
pub(crate) fn x86_state_imul_valid(op: &SmirOp) -> bool {
    decode_state_imul(op).is_some()
}

impl X86_64Lowerer {
    pub(crate) fn lower_state_imul(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        let shape = decode_state_imul(op).ok_or_else(|| LowerError::InvalidOperand {
            op: "state-backed IMUL".to_string(),
            operand: format!("invalid state-backed signed multiply: {:?}", op.kind),
        })?;
        let flags = match shape {
            X86StateImul::Implicit { flags, .. } | X86StateImul::Truncated { flags, .. } => flags,
        };
        let preserve_flags = flags == FlagUpdate::None;

        self.code.emit_u8(0x50); // preserve guest RAX while snapshotting
        self.emit_load_state_ptr_rax();
        if preserve_flags {
            self.code.emit_u8(0x9C); // guest RAX is now at [rsp+8]
        }
        self.emit_spill_legacy_gprs_to_state_from_rax(if preserve_flags { 8 } else { 0 });

        match shape {
            X86StateImul::Truncated {
                dst,
                src1,
                src2,
                width,
                ..
            } => {
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_rm(PhysReg::Rdx, PhysReg::Rax, i32::from(src1) * 8, width);
                    match src2 {
                        X86StateImulSource::Reg(source) => {
                            emitter.emit_mov_rm(
                                PhysReg::Rdi,
                                PhysReg::Rax,
                                i32::from(source) * 8,
                                width,
                            );
                            emitter.emit_imul_rr(PhysReg::Rdx, PhysReg::Rdi, width);
                        }
                        X86StateImulSource::Imm { value, use_imm8 } => {
                            emitter.emit_imul_rri_force(
                                PhysReg::Rdx,
                                PhysReg::Rdx,
                                value,
                                width,
                                use_imm8,
                            );
                        }
                    }
                }
                self.emit_store_gpr_slot_from_reg(dst, PhysReg::Rdx, width)?;
                if dst == 5 {
                    let commit_width = if width == OpWidth::W16 {
                        OpWidth::W16
                    } else {
                        OpWidth::W64
                    };
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_mov_mr(PhysReg::Rbp, 0, PhysReg::Rdx, commit_width);
                }
            }
            X86StateImul::Implicit { source, width, .. } => {
                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    // Retain the state pointer before architectural RAX becomes
                    // the implicit multiplicand.
                    emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
                    emitter.emit_mov_rm(PhysReg::Rdi, PhysReg::Rax, i32::from(source) * 8, width);
                    emitter.emit_mov_rm(PhysReg::Rax, PhysReg::Rax, 0, width);
                    emitter.emit_imul(PhysReg::Rdi, width);

                    let low_width = if width == OpWidth::W8 {
                        OpWidth::W16
                    } else {
                        width
                    };
                    emitter.emit_mov_rr(PhysReg::R8, PhysReg::Rax, low_width);
                    if width != OpWidth::W8 {
                        emitter.emit_mov_rr(PhysReg::R9, PhysReg::Rdx, width);
                    }
                    emitter.emit_mov_rr(PhysReg::Rax, PhysReg::Rcx, OpWidth::W64);
                }
                self.emit_store_gpr_slot_from_reg(
                    0,
                    PhysReg::R8,
                    if width == OpWidth::W8 {
                        OpWidth::W16
                    } else {
                        width
                    },
                )?;
                if width != OpWidth::W8 {
                    self.emit_store_gpr_slot_from_reg(2, PhysReg::R9, width)?;
                }
            }
        }

        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_mov_rr(PhysReg::Rcx, PhysReg::Rax, OpWidth::W64);
        }
        self.emit_reload_all(PhysReg::Rcx);
        if preserve_flags {
            self.code.emit_u8(0x9D);
        }
        self.emit_flag_preserving_stack_pop8();
        Ok(())
    }
}
