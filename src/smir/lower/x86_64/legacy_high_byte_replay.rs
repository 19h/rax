//! Exact scalar replay wrappers for legacy AH/CH/DH/BH encodings.

use super::*;
use crate::smir::ir::X86NativeReplaySpan;

impl X86_64Lowerer {
    /// Emit high-byte `CMPXCHG` with an explicit architectural flag image.
    /// Returns `false` when `span` belongs to another replay family.
    pub(crate) fn try_emit_legacy_high_byte_cmpxchg_replay(
        &mut self,
        span: &X86NativeReplaySpan,
    ) -> bool {
        let Some(destination) = span
            .instruction
            .legacy_high_byte_cmpxchg_destination_index()
        else {
            return false;
        };

        // Intel defines CMPXCHG's arithmetic flags as AL minus the
        // destination. Some translated x86-64 hosts instead publish the
        // reverse subtraction for AH/CH/DH/BH encodings. CMPXCHG consumes no
        // flags, so compute the specified image first, preserve it on the host
        // stack, execute the exact source bytes for their state transition,
        // and restore the comparison image afterwards.
        self.code.emit_bytes(&[0x3A, 0xC0 | destination]); // cmp al,r/m8
        self.code.emit_u8(0x9C); // pushfq
        self.code.emit_bytes(span.instruction.as_slice());
        self.code.emit_u8(0x9D); // popfq
        true
    }
}
