//! State-backed wrappers for exact native instructions using guest RSP/RBP.

use crate::smir::ir::X86InstructionBytes;
use crate::smir::lower::{X86_STATE_PTR_AT_RBP, x86_64::X86_64Lowerer};

impl X86_64Lowerer {
    /// Execute an exact vector-to-GPR replay whose architectural destination
    /// is guest RSP or RBP without exposing the host stack/frame register.
    pub(crate) fn emit_state_backed_gpr_replay(
        &mut self,
        rewritten: &X86InstructionBytes,
        destination: u8,
    ) {
        debug_assert!(matches!(destination, 4 | 5));

        self.code.emit_u8(0x50); // push rax
        self.code.emit_u8(0x51); // push rcx
        self.code.emit_bytes(rewritten.as_slice());
        self.code.emit_bytes(&[0x48, 0x8B, 0x4D]);
        self.code.emit_u8(X86_STATE_PTR_AT_RBP as u8); // mov rcx,[rbp+state]
        self.code.emit_bytes(&[0x48, 0x89, 0x41]);
        self.code.emit_u8(destination * 8); // mov [rcx+gpr[destination]],rax
        if destination == 5 {
            self.code.emit_bytes(&[0x48, 0x89, 0x45, 0x00]); // mov [rbp],rax
        }
        self.code.emit_u8(0x59); // pop rcx
        self.code.emit_u8(0x58); // pop rax
    }

    /// Execute an exact GPR-to-vector replay whose architectural source is
    /// guest RSP or RBP without exposing the host stack/frame register.
    pub(crate) fn emit_state_backed_gpr_source_replay(
        &mut self,
        rewritten: &X86InstructionBytes,
        source: u8,
    ) {
        debug_assert!(matches!(source, 4 | 5));

        self.code.emit_u8(0x50); // push rax
        self.code.emit_bytes(&[0x48, 0x8B, 0x45]);
        self.code.emit_u8(X86_STATE_PTR_AT_RBP as u8); // mov rax,[rbp+state]
        self.code.emit_bytes(&[0x48, 0x8B, 0x40, source * 8]); // mov rax,[rax+gpr]
        self.code.emit_bytes(rewritten.as_slice());
        self.code.emit_u8(0x58); // pop rax
    }
}
