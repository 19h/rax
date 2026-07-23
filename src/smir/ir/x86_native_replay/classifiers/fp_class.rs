//! Register-only EVEX VFPCLASS* replay classification.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one register-only EVEX VFPCLASS* instruction.
    ///
    /// Returns `(needs_avx512vl, needs_avx512dq, needs_avx512fp16)`. Packed
    /// 128-bit and 256-bit forms need AVX-512VL; binary32/binary64 forms need
    /// AVX-512DQ; binary16 forms need AVX-512-FP16. Scalar L'L is ignored and
    /// never creates an AVX-512VL requirement. Memory forms and every reserved
    /// EVEX field fail closed.
    pub fn evex_register_fp_class_requirements(&self) -> Option<(bool, bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 7 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p0 & 0x0F != 3
            || p0 & 0x90 != 0x90
            || p1 & 0x04 == 0
            || p1 & 0x78 != 0x78
            || p2 & 0x08 == 0
            || p2 & 0x90 != 0
            || !matches!(opcode, 0x66 | 0x67)
            || modrm >> 6 != 3
        {
            return None;
        }

        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let (needs_dq, needs_fp16) = match (pp, w) {
            (0, false) => (false, true),
            (1, _) => (true, false),
            _ => return None,
        };
        let scalar = opcode == 0x67;
        let ll = (p2 >> 5) & 0x03;
        if !scalar && ll == 3 {
            return None;
        }
        Some((!scalar && ll != 2, needs_dq, needs_fp16))
    }
}
