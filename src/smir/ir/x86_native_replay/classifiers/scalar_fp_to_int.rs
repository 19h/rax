//! Register-only EVEX scalar floating-point-to-integer replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only EVEX scalar floating-point-to-integer
    /// conversion and return whether it requires AVX-512-FP16.
    ///
    /// The admitted set is `VCVT{SS,SD,SH}2{SI,USI}` plus the corresponding
    /// truncating `VCVTT*` forms, for 32-bit and 64-bit integer destinations.
    /// Map-1 binary32/binary64 forms require AVX-512F; map-5 binary16 forms
    /// require AVX-512-FP16. `L'L` is ignored when `EVEX.b=0`, selects embedded
    /// rounding for non-truncating `EVEX.b=1` forms, and remains LLIG for the
    /// truncating SAE forms. Consequently `L'L=11` is valid only for the
    /// non-truncating embedded-rounding form.
    ///
    /// Memory sources, reserved vvvv/V'/mask/zeroing fields, fabricated GPR bit
    /// 4, and RSP/RBP destinations fail closed. RSP/RBP are unsafe because raw
    /// native replay identity-maps guest GPRs while the trampoline owns the host
    /// stack and frame registers.
    pub fn evex_register_scalar_fp_to_int_requires_fp16(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }

        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p1 & 0x04 == 0
            || p1 & 0x78 != 0x78
            || p2 & 0x8F != 0x08
            || modrm >> 6 != 3
            || p0 & 0x10 == 0
            || !matches!(opcode, 0x2C | 0x2D | 0x78 | 0x79)
        {
            return None;
        }

        let map = p0 & 0x0F;
        let pp = p1 & 0x03;
        let needs_fp16 = match (map, pp) {
            // EVEX.F3/F2.0F forms convert binary32/binary64 respectively.
            (1, 2 | 3) => false,
            // EVEX.F3.MAP5 forms convert binary16.
            (5, 2) => true,
            _ => return None,
        };

        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let truncating = matches!(opcode, 0x2C | 0x78);
        if ll == 3 && !(embedded_control && !truncating) {
            return None;
        }

        // EVEX.R' is reserved above. EVEX.R selects GPR0-7/GPR8-15; reject
        // guest RSP/RBP only in the low bank. The r/m extension channels name
        // XMM0-XMM31 and are all safe.
        let low_gpr_bank = p0 & 0x80 != 0;
        let gpr_low = (modrm >> 3) & 0x07;
        if low_gpr_bank && matches!(gpr_low, 4 | 5) {
            return None;
        }

        Some(needs_fp16)
    }
}
