//! Fault-precise helper-backed MXCSR memory operations.

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{ArchReg, MemWidth, OpWidth, SignExtend, VReg, VecWidth};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_GUEST_MXCSR_OFFSET};

use super::{X86_64Lowerer, X86Emitter};

/// Validate the exact legacy or VEX.LZ.WIG `STMXCSR` shape emitted by the
/// strict x86-64 lifter. The VEX W bit is architecturally ignored.
pub(crate) fn x86_store_mxcsr_shape_valid(op: &SmirOp) -> bool {
    let OpKind::X86StoreMxcsr { addr } = &op.kind else {
        return false;
    };
    // VEX cannot encode EGPR address operands, while the legacy IR shape does
    // not carry the preceding REX2/APX requirement inside this operation.
    // Reject both ambiguous cases until native admission can prove that guard.
    let uses_egpr = addr
        .regs()
        .iter()
        .any(|reg| matches!(reg, VReg::Arch(ArchReg::X86(x86)) if x86.is_egpr()));
    addr.is_x86_state_backed_shape()
        && !uses_egpr
        && matches!(
            op.x86_hint,
            None | Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::None,
                opcode: 0xAE,
                width: VecWidth::V128,
                ..
            })
        )
}

impl X86_64Lowerer {
    /// Store the current architectural MXCSR through the canonical 4-byte MMU
    /// helper. A helper failure restores every GPR and RFLAGS bit, removes the
    /// caller-owned staging slot, and exits at the faulting guest PC without a
    /// guest-memory commit.
    pub(crate) fn emit_x86_store_mxcsr(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.mem_helpers {
            return Err(LowerError::UnsupportedOp {
                op: "X86StoreMxcsr requires JIT MMU helpers".to_string(),
            });
        }
        if !x86_store_mxcsr_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86StoreMxcsr".to_string(),
                operand: "requires a legacy or VEX.LZ.WIG state-backed x86 address".to_string(),
            });
        }
        let OpKind::X86StoreMxcsr { addr } = &op.kind else {
            unreachable!("validated MXCSR store shape changed")
        };

        // Reserve one aligned caller slot without modifying flags. In a vector
        // region the live host MXCSR includes status accrued by preceding
        // native FP operations. Otherwise GuestRegs.mxcsr is authoritative and
        // can be staged with flag-neutral MOV/LEA bookkeeping.
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
        }
        // The store-helper ABI transports a u64 even though the architectural
        // access is 4 bytes. Clear the complete staged value first so its
        // unused high half is deterministic for both STMXCSR and MOV sources.
        self.code
            .emit_bytes(&[0x48, 0xC7, 0x04, 0x24, 0x00, 0x00, 0x00, 0x00]);
        if self.preserve_vector_mem_helpers {
            self.code.emit_bytes(&[0x0F, 0xAE, 0x1C, 0x24]); // stmxcsr [rsp]
        } else {
            self.code.emit_u8(0x50); // push guest RAX
            self.emit_load_state_ptr_rax();
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_mov_rm(
                    PhysReg::Rax,
                    PhysReg::Rax,
                    X86_GUEST_MXCSR_OFFSET,
                    OpWidth::W32,
                );
                // After POP, the caller slot begins at [rsp].
                emitter.emit_mov_mr(PhysReg::Rsp, 8, PhysReg::Rax, OpWidth::W32);
            }
            self.code.emit_u8(0x58); // pop guest RAX
        }

        // The generic helper pushes guest RAX and RFLAGS, so caller [rsp] is
        // visible as [rsp+16] while it collects the store value.
        self.emit_jit_mem_op(
            op.guest_pc,
            false,
            None,
            None,
            None,
            None,
            Some(16),
            addr,
            MemWidth::B4,
            SignExtend::Zero,
            16,
        )?;
        {
            let mut emitter = X86Emitter::new(&mut self.code);
            emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
        }
        Ok(())
    }
}
