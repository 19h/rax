//! Register-only EVEX classifiers for 128-bit and 256-bit chunk transforms.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate register-only EVEX VSHUFF32x4/VSHUFF64x2 and
    /// VSHUFI32x4/VSHUFI64x2, returning whether AVX-512VL is required. Only
    /// 256- and 512-bit vector lengths exist; memory, EVEX.b, malformed masks,
    /// incorrect prefixes/opcodes, and incorrect lengths fail closed.
    pub fn evex_register_chunk_shuffle_needs_vl(&self) -> Option<bool> {
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
            || p1 & 0x04 == 0
            || p1 & 0x03 != 1
            || !matches!(opcode, 0x23 | 0x43)
            || modrm >> 6 != 3
        {
            return None;
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_broadcast = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_broadcast || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only EVEX VINSERTF*/VINSERTI* vector-chunk forms.
    /// Returns `(needs_avx512vl, needs_avx512dq)`. The 128-bit chunk W1 forms
    /// and 256-bit chunk W0 forms require AVX-512DQ; their complementary forms
    /// require AVX-512F only. Memory, EVEX.b, illegal vector lengths, malformed
    /// masks, incorrect prefixes/opcodes, and incorrect lengths fail closed.
    pub fn evex_register_chunk_insert_requirements(&self) -> Option<(bool, bool)> {
        self.evex_register_chunk_requirements(&[0x18, 0x1A, 0x38, 0x3A], false)
    }

    /// Validate register-only EVEX VEXTRACTF*/VEXTRACTI* vector-chunk forms.
    /// Returns `(needs_avx512vl, needs_avx512dq)` with the same feature split as
    /// insertion. Memory destinations, EVEX.b, non-1111b EVEX.vvvv, cleared
    /// EVEX.V', illegal vector lengths, malformed masks, incorrect
    /// prefixes/opcodes, and incorrect lengths fail closed.
    pub fn evex_register_chunk_extract_requirements(&self) -> Option<(bool, bool)> {
        self.evex_register_chunk_requirements(&[0x19, 0x1B, 0x39, 0x3B], true)
    }

    fn evex_register_chunk_requirements(
        &self,
        opcodes: &[u8; 4],
        reserved_source: bool,
    ) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 7 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        let ll = (p2 >> 5) & 0x03;
        let half_chunk = matches!(opcode, 0x1A | 0x1B | 0x3A | 0x3B);

        if p0 & 0x0F != 3
            || p1 & 0x04 == 0
            || p1 & 0x03 != 1
            || !opcodes.contains(&opcode)
            || !matches!((half_chunk, ll), (false, 1 | 2) | (true, 2))
            || modrm >> 6 != 3
            || (reserved_source && (p1 & 0x78 != 0x78 || p2 & 0x08 == 0))
        {
            return None;
        }

        let zeroing = p2 & 0x80 != 0;
        let embedded_broadcast = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_broadcast || (zeroing && mask == 0) {
            return None;
        }
        let w = p1 & 0x80 != 0;
        Some((ll != 2, w != half_chunk))
    }
}
