//! Fused native lowering for LAHF and SAHF.

use super::*;

impl X86_64Lowerer {
    /// Replace the exact six-op AH/flags graph with its canonical host opcode.
    ///
    /// RAX and the modeled status flags are live in their architectural host
    /// locations while a JIT region runs. Replaying the instruction directly
    /// therefore preserves every non-status flag and every unrelated GPR while
    /// avoiding virtual-register allocation. The repository's x86-64-v3 host
    /// baseline includes LAHF/SAHF in 64-bit mode.
    #[cfg(feature = "smir-jit")]
    pub(crate) fn try_lower_jit_ah_flags(
        &mut self,
        block: &SmirBlock,
        idx: usize,
        virtual_definitions: &HashMap<VReg, usize>,
        virtual_uses: &HashMap<VReg, usize>,
    ) -> Result<Option<usize>, LowerError> {
        let Some(sequence) = crate::smir::lower::runtime::x86_jit_ah_flags_sequence(
            block,
            idx,
            virtual_definitions,
            virtual_uses,
        ) else {
            return Ok(None);
        };
        let opcode = match sequence.kind {
            crate::smir::lower::runtime::X86JitAhFlagsKind::Lahf => 0x9F,
            crate::smir::lower::runtime::X86JitAhFlagsKind::Sahf => 0x9E,
        };
        self.code.emit_u8(opcode);
        Ok(Some(sequence.consumed))
    }
}
