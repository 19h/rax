//! State-backed upper-vector maintenance for the AVX YMM0-YMM15 bridge.

use super::X86_64Lowerer;
use crate::smir::lower::{X86_GUEST_ZMM_OFFSET, X86_STATE_PTR_AT_RBP};

impl X86_64Lowerer {
    /// Preserve the architectural VEX zero-upper result when an instruction
    /// executes through the AVX-only YMM0-YMM15 entry bridge. That bridge
    /// deliberately leaves ZMM[511:256] state-backed; clear exactly the
    /// dynamically executed destination's four upper qwords before any later
    /// helper or native exit.
    ///
    /// PUSHFQ/PUSH RAX make the bookkeeping invisible to guest GPRs and flags.
    pub(crate) fn emit_avx_ymm16_state_backed_upper_clear(&mut self, destination: u8) {
        debug_assert!(destination < 16);
        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push rax
        self.code.emit_bytes(&[0x48, 0x8B, 0x45]);
        self.code.emit_u8(X86_STATE_PTR_AT_RBP as u8); // mov rax,[rbp+state]
        let upper = X86_GUEST_ZMM_OFFSET + i32::from(destination) * 64 + 32;
        for offset in (upper..upper + 32).step_by(8) {
            self.code.emit_bytes(&[0x48, 0xC7, 0x80]); // mov qword [rax+disp32],0
            self.code.emit_u32(offset as u32);
            self.code.emit_u32(0);
        }
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
    }

    /// Clear the state-backed ZMM[511:256] halves of registers 0-15 after an
    /// operandless VEX zero instruction executes through the AVX-only
    /// YMM0-YMM15 bridge. `VZEROUPPER` and `VZEROALL` both clear this range
    /// architecturally; the native instruction supplies the low-256-bit
    /// effect, while this 16-iteration loop supplies the state-backed part.
    ///
    /// The loop is O(16) time, O(1) space, and preserves guest RAX, RCX, and
    /// RFLAGS. It deliberately leaves registers 16-31 and opmask state intact.
    pub(crate) fn emit_avx_ymm16_state_backed_all_upper_clear(&mut self) {
        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_u8(0x50); // push rax
        self.code.emit_u8(0x51); // push rcx
        self.code.emit_bytes(&[0x48, 0x8B, 0x45]);
        self.code.emit_u8(X86_STATE_PTR_AT_RBP as u8); // mov rax,[rbp+state]
        self.code.emit_bytes(&[0x48, 0x8D, 0x80]); // lea rax,[rax+zmm0.upper]
        self.code.emit_u32((X86_GUEST_ZMM_OFFSET + 32) as u32);
        self.code.emit_u8(0xB9); // mov ecx,16
        self.code.emit_u32(16);

        let loop_start = self.code.position();
        for offset in [0u8, 8, 16, 24] {
            self.code.emit_bytes(&[0x48, 0xC7, 0x40, offset]); // mov qword [rax+disp8],0
            self.code.emit_u32(0);
        }
        self.code.emit_bytes(&[0x48, 0x83, 0xC0, 0x40]); // add rax,64
        self.code.emit_bytes(&[0xFF, 0xC9]); // dec ecx
        self.code.emit_u8(0x75); // jnz loop_start
        let next_ip = self.code.position() + 1;
        let displacement = i8::try_from(loop_start as isize - next_ip as isize)
            .expect("fixed ZMM upper-clear loop must fit a rel8 branch");
        self.code.emit_u8(displacement as u8);

        self.code.emit_u8(0x59); // pop rcx
        self.code.emit_u8(0x58); // pop rax
        self.code.emit_u8(0x9D); // popfq
    }
}
