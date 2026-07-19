//! Register- and flag-preserving SERIALIZE lowering.

use super::X86_64Lowerer;

impl X86_64Lowerer {
    /// CPUID is available on every x86-64 host and is a serializing
    /// instruction. Execute fixed leaf zero only as a barrier, preserving all
    /// four clobbered GPRs and the complete live host flags image.
    pub(crate) fn emit_x86_serialize(&mut self) {
        self.code.emit_bytes(&[
            0x9C, // pushfq
            0x50, // push rax
            0x53, // push rbx
            0x51, // push rcx
            0x52, // push rdx
            0xB8, 0x00, 0x00, 0x00, 0x00, // mov eax,0
            0x0F, 0xA2, // cpuid
            0x5A, // pop rdx
            0x59, // pop rcx
            0x5B, // pop rbx
            0x58, // pop rax
            0x9D, // popfq
        ]);
    }
}
