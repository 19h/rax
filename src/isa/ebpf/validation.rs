//! Encoding and destination constraints shared by direct execution and SMIR.

use super::{AtomicKind, EbpfDecodeError, EbpfDialect, EbpfInsn, EbpfOp, EbpfSrc};

pub(super) fn validate(insn: &EbpfInsn, dialect: EbpfDialect) -> Result<(), EbpfDecodeError> {
    let EbpfInsn {
        opc,
        dst,
        src,
        off,
        imm,
        ..
    } = *insn;
    let reserved = || EbpfDecodeError::ReservedField { opc };

    if dialect == EbpfDialect::Rfc9669 {
        // RFC 9669 section 3.1: every unused field must be zero. Fields
        // repurposed by DIV/SDIV, MOD/SMOD, MOVSX and END remain meaningful.
        let valid = match insn.op {
            EbpfOp::LoadImm64 { src_type, .. } => {
                off == 0 && (!matches!(src_type, 1 | 3 | 4 | 5) || insn.imm2 == 0)
            }
            EbpfOp::LoadReg { .. } | EbpfOp::StoreReg { .. } => imm == 0,
            EbpfOp::StoreImm { .. } => src == 0,
            EbpfOp::Atomic { .. } => true,
            EbpfOp::BinAlu { src: operand, .. } => {
                let used_offset = matches!(opc & 0xf0, 0x30 | 0x90 | 0xb0);
                (used_offset || off == 0)
                    && match operand {
                        EbpfSrc::Reg(_) => imm == 0,
                        EbpfSrc::Imm(_) => src == 0,
                    }
            }
            EbpfOp::Neg { .. } => src == 0 && off == 0 && imm == 0,
            EbpfOp::Endian { .. } => src == 0 && off == 0,
            EbpfOp::Jump { .. } => {
                dst == 0 && src == 0 && if opc == 0x05 { imm == 0 } else { off == 0 }
            }
            EbpfOp::JumpCond { src: operand, .. } => match operand {
                EbpfSrc::Reg(_) => imm == 0,
                EbpfSrc::Imm(_) => src == 0,
            },
            EbpfOp::CallImm { .. } => dst == 0 && off == 0,
            EbpfOp::CallReg { .. } => return Err(EbpfDecodeError::InvalidOpcode { opc }),
            EbpfOp::Exit => dst == 0 && src == 0 && off == 0 && imm == 0,
        };
        if !valid {
            return Err(reserved());
        }
    }

    let written = match insn.op {
        EbpfOp::LoadImm64 { dst, .. }
        | EbpfOp::LoadReg { dst, .. }
        | EbpfOp::BinAlu { dst, .. }
        | EbpfOp::Neg { dst, .. }
        | EbpfOp::Endian { dst, .. } => Some(dst),
        EbpfOp::Atomic { src, kind, .. } => match kind {
            AtomicKind::Add { fetch: true }
            | AtomicKind::Or { fetch: true }
            | AtomicKind::And { fetch: true }
            | AtomicKind::Xor { fetch: true }
            | AtomicKind::Xchg => Some(src),
            _ => None,
        },
        _ => None,
    };
    if written == Some(10) {
        // Anza verifier::check_registers: the sole writable-r10 form is
        // ADD64_IMM in the two manual-frame versions, V1 and V2.
        let manual_adjustment = opc == 0x07
            && dialect
                .sbpf_version()
                .is_some_and(|v| v.manual_stack_frame_bump());
        if !manual_adjustment {
            return Err(reserved());
        }
    }
    if let EbpfOp::CallReg { target_reg: 10 } = insn.op {
        return Err(EbpfDecodeError::InvalidRegister { reg: 10 });
    }
    Ok(())
}
