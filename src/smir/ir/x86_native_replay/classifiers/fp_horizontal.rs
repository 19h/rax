//! Register-only x86 packed floating-point horizontal/add-sub replay
//! classification.

use super::X86InstructionBytes;

const OPCODES: [u8; 3] = [0x7C, 0x7D, 0xD0];

impl X86InstructionBytes {
    /// Validate one register-only legacy SSE3 or AVX VEX
    /// `HADD`/`HSUB`/`ADDSUB` packed binary32/binary64 instruction and report
    /// whether it requires AVX rather than SSE3.
    ///
    /// Binary64 forms use mandatory prefix 66; binary32 forms use F2. VEX
    /// forms use map 0F, specify WIG, and admit both 128-bit and 256-bit vector
    /// lengths. Memory operands and every malformed or non-canonical byte
    /// shape fail closed.
    pub fn legacy_vex_register_fp_horizontal_addsub_needs_avx(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let legacy_modrm = match bytes {
            [0x66 | 0xF2, 0x0F, opcode, modrm] if OPCODES.contains(opcode) => Some(*modrm),
            [0x66 | 0xF2, 0x40..=0x4F, 0x0F, opcode, modrm] if OPCODES.contains(opcode) => {
                Some(*modrm)
            }
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
        (OPCODES.contains(&opcode) && matches!(p1 & 0x03, 1 | 3) && modrm >> 6 == 3).then_some(true)
    }
}
