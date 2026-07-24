//! Register-only EVEX scalar-integer move replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only EVEX scalar-integer move that is not already
    /// directly lowerable from its semantic SMIR operation.
    ///
    /// The admitted set is exactly the two XMM-to-XMM `VMOVQ` aliases and the
    /// two GPR-to/from-XMM `VMOVW` aliases. `VMOVQ` requires AVX-512F;
    /// `VMOVW` requires AVX-512-FP16, which is returned as `true`. Both sets
    /// are fixed at EVEX.128, reserve vvvv/V', masking, zeroing, and EVEX.b,
    /// and have no floating-point exception behavior. `VMOVW` additionally
    /// rejects RSP/RBP because native replay identity-maps guest GPRs while
    /// those two physical registers hold the host stack and frame state.
    pub fn evex_register_scalar_integer_move_requires_fp16(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }

        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p1 & 0x04 == 0 || p1 & 0x78 != 0x78 || p2 != 0x08 || modrm >> 6 != 3 {
            return None;
        }

        let map = p0 & 0x0f;
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        if map == 1 && w && matches!((pp, opcode), (2, 0x7E) | (1, 0xD6)) {
            // All four P0 extension channels name XMM registers for these
            // aliases, so every XMM0-XMM31 source/destination is replayable.
            return Some(false);
        }

        if map != 5 || pp != 1 || !matches!(opcode, 0x6E | 0x7E) {
            return None;
        }

        // VMOVW's ModR/M r/m operand is a 16-register GPR. The lifter rejects
        // EVEX.X as a fabricated bit 4, and the identity-map trampoline cannot
        // expose guest RSP/RBP as native sources or destinations.
        if p0 & 0x40 == 0 {
            return None;
        }
        let gpr_low = modrm & 0x07;
        let low_gpr_bank = p0 & 0x20 != 0;
        if low_gpr_bank && matches!(gpr_low, 4 | 5) {
            return None;
        }

        Some(true)
    }
}
