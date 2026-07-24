//! Register-only EVEX scalar integer-to-floating-point replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only EVEX scalar integer-to-floating-point
    /// conversion and return whether it requires AVX-512-FP16.
    ///
    /// The admitted set is `VCVT{,U}SI2{SS,SD,SH}` for 32-bit and 64-bit GPR
    /// sources. Map-1 binary32/binary64 forms require AVX-512F; map-5 binary16
    /// forms require AVX-512-FP16. `L'L` is ignored when `EVEX.b=0` and selects
    /// embedded rounding/SAE when `EVEX.b=1`, except that binary64 W0 is exact
    /// for every input and architecturally ignores attempted embedded rounding.
    /// All four `L'L` values therefore remain valid.
    ///
    /// Memory sources, masks, zeroing, fabricated GPR bit 4, and RSP/RBP
    /// sources fail closed. RSP/RBP are unsafe because raw native replay
    /// identity-maps guest GPRs while the trampoline owns the host stack and
    /// frame registers.
    pub fn evex_register_scalar_int_to_fp_requires_fp16(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }

        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        // EVEX.X would fabricate a fifth GPR source bit. EVEX.R/R' select the
        // 32 architectural XMM destinations and are both meaningful here.
        if p1 & 0x04 == 0
            || p2 & 0x87 != 0
            || modrm >> 6 != 3
            || p0 & 0x40 == 0
            || !matches!(opcode, 0x2A | 0x7B)
        {
            return None;
        }

        let map = p0 & 0x0F;
        let pp = p1 & 0x03;
        let needs_fp16 = match (map, pp) {
            // EVEX.F3/F2.0F forms produce binary32/binary64 respectively.
            (1, 2 | 3) => false,
            // EVEX.F3.MAP5 forms produce binary16.
            (5, 2) => true,
            _ => return None,
        };

        // EVEX.B selects GPR0-7/GPR8-15. Reject guest RSP/RBP only in the low
        // bank; R12/R13 are ordinary identity-mapped source registers.
        let low_gpr_bank = p0 & 0x20 != 0;
        let gpr_low = modrm & 0x07;
        if low_gpr_bank && matches!(gpr_low, 4 | 5) {
            return None;
        }

        Some(needs_fp16)
    }
}
