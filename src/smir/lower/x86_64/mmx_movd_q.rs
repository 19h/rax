//! Exact native MMX scalar and MMX/XMM transfer encodings.

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix};
use crate::smir::ir::types::{ArchReg, DispSize, OpWidth, VReg, X86Reg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_GUEST_ZMM_OFFSET};

use super::{X86_64Lowerer, X86Emitter};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum X86NativeMmxMovdQEncoding {
    Gpr {
        mm: VReg,
        gpr: VReg,
        opcode: u8,
        width: OpWidth,
    },
    Xmm {
        mm_index: u8,
        xmm_index: u8,
        xmm_destination: bool,
    },
}

fn mm_index(reg: VReg) -> Option<u8> {
    match reg {
        VReg::Arch(ArchReg::X86(X86Reg::Mm(index @ 0..=7))) => Some(index),
        _ => None,
    }
}

fn low_xmm_index(reg: VReg) -> Option<u8> {
    match reg {
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))) => Some(index),
        _ => None,
    }
}

fn safe_gpr(reg: VReg) -> bool {
    matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some_and(|index| index <= 15 && !matches!(index, 4 | 5)))
}

fn x86_native_mmx_movd_q_encoding(op: &SmirOp) -> Option<X86NativeMmxMovdQEncoding> {
    let OpKind::X86MovdQ {
        dst,
        src,
        width,
        zero_upper,
    } = &op.kind
    else {
        return None;
    };
    if *zero_upper {
        return None;
    }

    if let (Some(mm_index), Some(xmm_index)) = (mm_index(*src), low_xmm_index(*dst)) {
        return (*width == OpWidth::W64
            && matches!(
                op.x86_hint,
                Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::Rep,
                    opcode: 0xD6,
                })
            ))
        .then_some(X86NativeMmxMovdQEncoding::Xmm {
            mm_index,
            xmm_index,
            xmm_destination: true,
        });
    }
    if let (Some(mm_index), Some(xmm_index)) = (mm_index(*dst), low_xmm_index(*src)) {
        return (*width == OpWidth::W64
            && matches!(
                op.x86_hint,
                Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::Repne,
                    opcode: 0xD6,
                })
            ))
        .then_some(X86NativeMmxMovdQEncoding::Xmm {
            mm_index,
            xmm_index,
            xmm_destination: false,
        });
    }

    let (mm, gpr, opcode) = if mm_index(*dst).is_some() && safe_gpr(*src) {
        (*dst, *src, 0x6E)
    } else if mm_index(*src).is_some() && safe_gpr(*dst) {
        (*src, *dst, 0x7E)
    } else {
        return None;
    };
    (matches!(width, OpWidth::W32 | OpWidth::W64)
        && matches!(
            op.x86_hint,
            Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: actual,
            }) if actual == opcode
        ))
    .then_some(X86NativeMmxMovdQEncoding::Gpr {
        mm,
        gpr,
        opcode,
        width: *width,
    })
}

pub(crate) fn x86_native_mmx_movd_q_candidate(op: &SmirOp) -> bool {
    matches!(
        &op.kind,
        OpKind::X86MovdQ { dst, src, .. }
            if matches!(dst, VReg::Arch(ArchReg::X86(X86Reg::Mm(_))))
                || matches!(src, VReg::Arch(ArchReg::X86(X86Reg::Mm(_))))
    )
}

pub(crate) fn x86_native_mmx_movd_q_shape_valid(op: &SmirOp) -> bool {
    x86_native_mmx_movd_q_encoding(op).is_some()
}

pub(crate) fn x86_mmx_xmm_transfer_shape_valid(op: &SmirOp) -> bool {
    matches!(
        x86_native_mmx_movd_q_encoding(op),
        Some(X86NativeMmxMovdQEncoding::Xmm { .. })
    )
}

impl X86_64Lowerer {
    pub(crate) fn lower_native_mmx_movd_q(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        let Some(encoding) = x86_native_mmx_movd_q_encoding(op) else {
            return Err(LowerError::InvalidOperand {
                op: "MMX MOVD/MOVQ transfer".to_string(),
                operand: "requires an exact MM/GPR or MM/XMM register encoding".to_string(),
            });
        };

        match encoding {
            X86NativeMmxMovdQEncoding::Gpr {
                mm,
                gpr,
                opcode,
                width,
            } => {
                let mm_reg = self.get_reg(mm)?;
                let gpr_reg = self.get_reg(gpr)?;
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mmx_movd_q_rr(opcode, mm_reg, gpr_reg, width);
            }
            X86NativeMmxMovdQEncoding::Xmm {
                mm_index,
                xmm_index,
                xmm_destination,
            } => {
                let mm = PhysReg::Mm(mm_index);
                let xmm = PhysReg::Xmm(xmm_index);
                let state_offset = X86_GUEST_ZMM_OFFSET + i32::from(xmm_index) * 64;

                // A region with no independently admitted native vector work
                // keeps XMM/ZMM authoritative in GuestRegs. Import only the
                // source low 128 bits before MOVDQ2Q; MOVQ2DQ fully defines its
                // low 128-bit destination and therefore needs no prior load.
                if !self.native_vector_state_active && !xmm_destination {
                    self.code.emit_u8(0x50); // push guest RAX
                    self.emit_load_state_ptr_rax();
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rm_disp(
                        Some(0xF3),
                        0x6F,
                        xmm,
                        PhysReg::Rax,
                        state_offset,
                        DispSize::Disp32,
                    );
                    self.code.emit_u8(0x58); // pop guest RAX
                }

                {
                    let mut emitter = X86Emitter::new(&mut self.code);
                    if xmm_destination {
                        emitter.emit_sse_mov_rr(Some(0xF3), 0xD6, xmm, mm);
                    } else {
                        emitter.emit_sse_mov_rr(Some(0xF2), 0xD6, mm, xmm);
                    }
                }

                // Preserve the shared YMM/ZMM backing above bit 127 by storing
                // exactly the architecturally written 16-byte XMM destination.
                if !self.native_vector_state_active && xmm_destination {
                    self.code.emit_u8(0x50); // push guest RAX
                    self.emit_load_state_ptr_rax();
                    let mut emitter = X86Emitter::new(&mut self.code);
                    emitter.emit_sse_mov_rm_disp(
                        Some(0xF3),
                        0x7F,
                        xmm,
                        PhysReg::Rax,
                        state_offset,
                        DispSize::Disp32,
                    );
                    self.code.emit_u8(0x58); // pop guest RAX
                }
            }
        }
        Ok(())
    }
}
