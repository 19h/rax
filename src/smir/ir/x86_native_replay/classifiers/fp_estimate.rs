//! Register-only legacy SSE and AVX VEX reciprocal-estimate classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only legacy SSE or AVX VEX reciprocal estimate
    /// and report whether it requires AVX.
    ///
    /// The admitted set is `RCPPS`, `RCPSS`, `RSQRTPS`, `RSQRTSS` and their
    /// VEX forms. Packed VEX forms reserve `vvvv`; scalar VEX forms use it as
    /// the upper-lane merge source. Intel specifies scalar `VEX.LIG`, so both
    /// encoded L values are admitted. C4 W and register-form X are ignored.
    /// Memory sources and every non-exact byte string fail closed.
    pub fn legacy_vex_register_fp_estimate_needs_avx(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let legacy_modrm = match bytes {
            [0x0F, 0x52 | 0x53, modrm] => Some(*modrm),
            [0xF3, 0x0F, 0x52 | 0x53, modrm] => Some(*modrm),
            [0x40..=0x4F, 0x0F, 0x52 | 0x53, modrm] => Some(*modrm),
            [0xF3, 0x40..=0x4F, 0x0F, 0x52 | 0x53, modrm] => Some(*modrm),
            _ => None,
        };
        if let Some(modrm) = legacy_modrm {
            return (modrm >> 6 == 3).then_some(false);
        }

        let (p1, opcode, modrm) = match bytes {
            [0xC5, p1, opcode, modrm] => (*p1, *opcode, *modrm),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (*p1, *opcode, *modrm),
            _ => return None,
        };
        if !matches!(opcode, 0x52 | 0x53) || modrm >> 6 != 3 {
            return None;
        }

        match p1 & 0x03 {
            0 if p1 & 0x78 == 0x78 => Some(true),
            2 => Some(true),
            _ => None,
        }
    }

    /// Return the architectural destination of a validated VEX reciprocal
    /// estimate. Legacy forms return `None` because they preserve all vector
    /// state above XMM and require no state-backed upper clear.
    pub(crate) fn vex_fp_estimate_destination_index(&self) -> Option<u8> {
        self.legacy_vex_register_fp_estimate_needs_avx()?;
        match self.as_slice() {
            [0xC5, p1, _opcode, modrm] => {
                Some(((modrm >> 3) & 7) | (u8::from(p1 & 0x80 == 0) << 3))
            }
            [0xC4, p0, _p1, _opcode, modrm] => {
                Some(((modrm >> 3) & 7) | (u8::from(p0 & 0x80 == 0) << 3))
            }
            _ => None,
        }
    }
}
