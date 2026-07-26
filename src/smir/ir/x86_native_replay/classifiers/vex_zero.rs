//! Operandless VEX zero-register replay.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one operandless AVX `VZEROUPPER`/`VZEROALL` instruction and
    /// return whether it is `VZEROALL`.
    ///
    /// Both encodings use VEX map 0F, opcode 77, no mandatory prefix, and the
    /// reserved on-wire `VEX.vvvv=1111b` value. `VEX.L=0` selects
    /// `VZEROUPPER`; `VEX.L=1` selects `VZEROALL`. The two- and three-byte VEX
    /// forms are legal, W is ignored in the latter, and the operandless R/X/B
    /// extension fields are ignored. Any trailing byte or malformed prefix
    /// fails closed.
    pub fn vex_zeroes_all_register_bits(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let p1 = match bytes {
            &[0xC5, p1, 0x77] => p1,
            &[0xC4, p0, p1, 0x77] if p0 & 0x1F == 1 => p1,
            _ => return None,
        };
        if p1 & 0x7B != 0x78 {
            return None;
        }
        Some(p1 & 0x04 != 0)
    }
}
