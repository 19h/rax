//! Byte-validated native replay metadata for x86 instructions.
//!
//! These classifiers accept exact register-only instruction shapes whose
//! source bytes can safely replace the contiguous semantic SMIR group emitted
//! for the same guest instruction.

use std::collections::HashMap;

use super::SmirBlock;
use super::types::{BlockId, GuestAddr};

/// Exact bytes of one x86 instruction. Architectural x86 instructions are at
/// most 15 bytes; keeping a fixed-size value makes function provenance cheap to
/// clone and prevents metadata from carrying an unbounded byte sequence into a
/// native lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct X86InstructionBytes {
    bytes: [u8; 15],
    len: u8,
}

impl X86InstructionBytes {
    /// Capture one complete x86 instruction.
    pub fn new(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() || bytes.len() > 15 {
            return None;
        }
        let mut captured = [0u8; 15];
        captured[..bytes.len()].copy_from_slice(bytes);
        Some(Self {
            bytes: captured,
            len: bytes.len() as u8,
        })
    }

    /// Return the complete instruction byte sequence.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    /// Validate the initial native-replay family and return whether its vector
    /// length requires AVX-512VL in addition to AVX-512F. The admitted set is
    /// exactly register-source EVEX VADD*/VMUL*/VSUB*/VMIN*/VDIV*/VMAX* over
    /// binary32/binary64 packed or scalar elements, without EVEX.b embedded
    /// rounding/SAE. Every structural and reserved field relevant to this set
    /// is checked so fabricated metadata fails closed.
    pub fn evex_register_fp_arithmetic_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        // Map 1 (0F), EVEX.P1's fixed-one bit, and a register ModR/M source.
        if p0 & 0x0f != 1 || p1 & 0x04 == 0 || modrm >> 6 != 3 {
            return None;
        }
        if !matches!(opcode, 0x58 | 0x59 | 0x5c | 0x5d | 0x5e | 0x5f) {
            return None;
        }

        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        // PS/SS use W0; PD/SD use W1.
        if w != matches!(pp, 1 | 3) {
            return None;
        }
        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }

        let scalar = matches!(pp, 2 | 3);
        if scalar {
            (ll == 0).then_some(false)
        } else {
            match ll {
                0 | 1 => Some(true),
                2 => Some(false),
                _ => None,
            }
        }
    }

    /// Validate register-only EVEX packed logical operations and return
    /// `(needs AVX-512VL, needs AVX-512DQ)`. Floating logical VAND*/VANDN*/
    /// VOR*/VXOR* forms use AVX-512DQ; integer VPANDD/Q, VPANDND/Q, VPORD/Q,
    /// and VPXORD/Q forms use AVX-512F. Memory, EVEX.b, reserved vector lengths,
    /// and malformed masking encodings are rejected.
    pub fn evex_register_logic_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 1 || p1 & 0x04 == 0 || modrm >> 6 != 3 {
            return None;
        }

        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let needs_avx512dq = match opcode {
            0x54..=0x57 if matches!(pp, 0 | 1) && w == (pp == 1) => true,
            0xDB | 0xDF | 0xEB | 0xEF if pp == 1 => false,
            _ => return None,
        };
        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        let needs_avx512vl = match ll {
            0 | 1 => true,
            2 => false,
            _ => return None,
        };
        Some((needs_avx512vl, needs_avx512dq))
    }

    /// Validate register-only EVEX packed integer additions/subtractions and
    /// return whether the vector length requires AVX-512VL. Byte/word and all
    /// saturating forms use AVX-512BW; doubleword/quadword wrapping forms use
    /// AVX-512F. The native vector-state trampoline already requires both
    /// feature sets, so only the additional VL requirement is returned here.
    /// Memory, EVEX.b, reserved vector lengths, and malformed masks fail closed.
    pub fn evex_register_integer_arithmetic_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 1 || p1 & 0x04 == 0 || modrm >> 6 != 3 || p1 & 0x03 != 1 {
            return None;
        }

        let w = p1 & 0x80 != 0;
        match opcode {
            // VPADDQ and VPSUBQ are W1; VPADDD and VPSUBD are W0.
            0xD4 | 0xFB if w => {}
            0xFA | 0xFE if !w => {}
            // Byte/word operations specify WIG.
            0xD8 | 0xD9 | 0xDC | 0xDD | 0xE8 | 0xE9 | 0xEC | 0xED | 0xF8 | 0xF9 | 0xFC | 0xFD => {}
            _ => return None,
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only EVEX packed shifts with a shared XMM count and
    /// return whether the destination vector length requires AVX-512VL.
    /// Word forms use AVX-512BW and doubleword/quadword forms use AVX-512F;
    /// both are already required by the native vector-state trampoline.
    pub fn evex_register_shared_count_shift_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 1 || p1 & 0x04 == 0 || modrm >> 6 != 3 || p1 & 0x03 != 1 {
            return None;
        }

        let w = p1 & 0x80 != 0;
        match opcode {
            // Word shifts are WIG; E2 selects VPSRAD/VPSRAQ by W.
            0xD1 | 0xE1 | 0xE2 | 0xF1 => {}
            // Doubleword shifts are W0; quadword shifts are W1.
            0xD2 | 0xF2 if !w => {}
            0xD3 | 0xF3 if w => {}
            _ => return None,
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only EVEX packed shifts with an immediate count and
    /// return whether the destination vector length requires AVX-512VL.
    /// Word forms use AVX-512BW and doubleword/quadword forms use AVX-512F;
    /// both are already required by the native vector-state trampoline.
    pub fn evex_register_immediate_count_shift_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 7 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 1 || p1 & 0x04 == 0 || modrm >> 6 != 3 || p1 & 0x03 != 1 {
            return None;
        }

        let w = p1 & 0x80 != 0;
        let extension = (modrm >> 3) & 0x07;
        match (opcode, extension) {
            // Word shifts are WIG.
            (0x71, 2 | 4 | 6) => {}
            // Doubleword shifts are W0; VPSRAQ is the W1 /4 form.
            (0x72, 2 | 4 | 6) if !w => {}
            (0x72, 4) if w => {}
            // Quadword logical shifts are W1.
            (0x73, 2 | 6) if w => {}
            _ => return None,
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only EVEX packed binary32/binary64 fused
    /// multiply-add/subtract operations and return whether the vector length
    /// requires AVX-512VL. Memory and EVEX.b embedded-rounding forms are
    /// intentionally excluded so replay remains register-only and uses the
    /// guest MXCSR rounding mode.
    pub fn evex_register_packed_fma_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 2 || p1 & 0x04 == 0 || modrm >> 6 != 3 || p1 & 0x03 != 1 {
            return None;
        }
        if !matches!(
            opcode,
            0x96..=0x98
                | 0x9A
                | 0x9C
                | 0x9E
                | 0xA6..=0xA8
                | 0xAA
                | 0xAC
                | 0xAE
                | 0xB6..=0xB8
                | 0xBA
                | 0xBC
                | 0xBE
        ) {
            return None;
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only EVEX scalar binary32/binary64 fused
    /// multiply-add/subtract operations. Scalar AVX-512 FMA forms use
    /// AVX-512F without AVX-512VL. Memory and EVEX.b embedded-rounding forms
    /// are intentionally excluded so replay uses the guest MXCSR rounding
    /// mode, and LLIG is admitted only in its canonical L'L=0 encoding.
    pub fn evex_register_scalar_fma_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 2 || p1 & 0x04 == 0 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }
        if !matches!(
            opcode,
            0x99 | 0x9B | 0x9D | 0x9F | 0xA9 | 0xAB | 0xAD | 0xAF | 0xB9 | 0xBB | 0xBD | 0xBF
        ) {
            return None;
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || ll != 0 || (zeroing && mask == 0) {
            return None;
        }
        Some(false)
    }

    /// Validate register-only EVEX packed binary16 fused
    /// multiply-add/subtract operations and return whether the vector length
    /// requires AVX-512VL. Every admitted instruction additionally requires
    /// AVX-512-FP16. Memory and EVEX.b embedded-rounding forms are
    /// intentionally excluded so replay remains register-only and uses the
    /// guest MXCSR rounding mode.
    pub fn evex_register_packed_fp16_fma_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 6 || p1 & 0x04 == 0 || p1 & 0x80 != 0 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }
        if !matches!(
            opcode,
            0x96..=0x98
                | 0x9A
                | 0x9C
                | 0x9E
                | 0xA6..=0xA8
                | 0xAA
                | 0xAC
                | 0xAE
                | 0xB6..=0xB8
                | 0xBA
                | 0xBC
                | 0xBE
        ) {
            return None;
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only EVEX scalar binary16 fused
    /// multiply-add/subtract operations. Scalar AVX-512-FP16 forms do not
    /// require AVX-512VL. Memory and EVEX.b embedded-rounding forms are
    /// intentionally excluded so replay uses the guest MXCSR rounding mode,
    /// and LLIG is admitted only in its canonical L'L=0 encoding.
    pub fn evex_register_scalar_fp16_fma_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 6 || p1 & 0x04 == 0 || p1 & 0x80 != 0 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }
        if !matches!(
            opcode,
            0x99 | 0x9B | 0x9D | 0x9F | 0xA9 | 0xAB | 0xAD | 0xAF | 0xB9 | 0xBB | 0xBD | 0xBF
        ) {
            return None;
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || ll != 0 || (zeroing && mask == 0) {
            return None;
        }
        Some(false)
    }

    /// Validate register-only scalar AVX-512-FP16 arithmetic and square-root
    /// instructions. VADDSH, VMULSH, VSUBSH, VMINSH, VDIVSH, VMAXSH, and
    /// VSQRTSH require AVX-512-FP16 but not AVX-512VL. Their L'L field is LLIG;
    /// for register sources EVEX.b either supplies embedded rounding or SAE, so
    /// every L'L/EVEX.b combination is retained verbatim for native replay.
    /// Memory forms and malformed zeroing-with-k0 encodings fail closed.
    pub fn evex_register_scalar_fp16_arithmetic_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        // MAP5, W0, mandatory F3, EVEX.P1 fixed-one, register ModR/M.
        if p0 & 0x0F != 5 || p1 & 0x87 != 0x06 || modrm >> 6 != 3 {
            return None;
        }
        if !matches!(opcode, 0x51 | 0x58 | 0x59 | 0x5C..=0x5F) {
            return None;
        }

        let zeroing = p2 & 0x80 != 0;
        let mask = p2 & 0x07;
        if zeroing && mask == 0 {
            return None;
        }
        Some(false)
    }

    /// Validate register-only EVEX packed signed/unsigned integer minimum and
    /// maximum operations and return whether the vector length requires
    /// AVX-512VL. Byte/word forms use AVX-512BW and doubleword/quadword forms
    /// use AVX-512F; both are required by the native vector-state trampoline.
    pub fn evex_register_integer_minmax_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p1 & 0x04 == 0 || modrm >> 6 != 3 || p1 & 0x03 != 1 {
            return None;
        }

        let map = p0 & 0x0f;
        if !matches!(
            (map, opcode),
            (1, 0xDA | 0xDE | 0xEA | 0xEE) | (2, 0x38..=0x3F)
        ) {
            return None;
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only EVEX packed integer multiply operations and
    /// return `(needs AVX-512VL, needs AVX-512DQ)`. `VPMULLQ` requires
    /// AVX-512DQ; the remaining admitted word/doubleword/quadword products use
    /// AVX-512F or AVX-512BW, both required by the vector-state trampoline.
    pub fn evex_register_integer_multiply_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p1 & 0x04 == 0 || modrm >> 6 != 3 || p1 & 0x03 != 1 {
            return None;
        }

        let map = p0 & 0x0f;
        let w = p1 & 0x80 != 0;
        let needs_avx512dq = match (map, opcode) {
            (1, 0xD5 | 0xE4 | 0xE5) | (2, 0x0B) => false,
            (1, 0xF4) | (2, 0x28) if w => false,
            (2, 0x40) => w,
            _ => return None,
        };

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        let needs_avx512vl = match ll {
            0 | 1 => true,
            2 => false,
            _ => return None,
        };
        Some((needs_avx512vl, needs_avx512dq))
    }

    /// Validate register-only EVEX packed integer low/high interleave
    /// operations and return whether the vector length requires AVX-512VL.
    /// Byte/word forms use AVX-512BW and doubleword/quadword forms use
    /// AVX-512F; both are required by the native vector-state trampoline.
    /// Memory/broadcast, EVEX.b, reserved vector lengths, and malformed masks
    /// fail closed.
    pub fn evex_register_integer_interleave_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 1 || p1 & 0x04 == 0 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }

        let w = p1 & 0x80 != 0;
        match opcode {
            // Byte/word forms specify WIG.
            0x60 | 0x61 | 0x68 | 0x69 => {}
            // Doubleword forms are W0; quadword forms are W1.
            0x62 | 0x6A if !w => {}
            0x6C | 0x6D if w => {}
            _ => return None,
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only EVEX signed/unsigned saturating pack operations
    /// and return whether the vector length requires AVX-512VL. All admitted
    /// forms require AVX-512BW. Byte-result forms specify WIG, while
    /// doubleword-to-word forms require W0. Memory/broadcast, EVEX.b, reserved
    /// vector lengths, and malformed masks fail closed.
    pub fn evex_register_integer_pack_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p1 & 0x04 == 0 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }

        let map = p0 & 0x0f;
        let w = p1 & 0x80 != 0;
        match (map, opcode) {
            // VPACKSSWB and VPACKUSWB specify WIG.
            (1, 0x63 | 0x67) => {}
            // VPACKSSDW and VPACKUSDW require W0.
            (1, 0x6B) | (2, 0x2B) if !w => {}
            _ => return None,
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only EVEX packed integer absolute-value operations
    /// and return whether the vector length requires AVX-512VL. Byte/word
    /// forms specify WIG, doubleword forms require W0, and quadword forms
    /// require W1. Reserved unary EVEX.vvvv/V', memory/broadcast, EVEX.b,
    /// reserved vector lengths, and malformed masks fail closed.
    pub fn evex_register_packed_abs_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 2 || p1 & 0x7f != 0x7d || p2 & 0x08 == 0 || modrm >> 6 != 3 {
            return None;
        }

        let w = p1 & 0x80 != 0;
        match opcode {
            // VPABSB and VPABSW specify WIG.
            0x1C | 0x1D => {}
            0x1E if !w => {}
            0x1F if w => {}
            _ => return None,
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only EVEX rounded unsigned packed byte/word average
    /// operations and return whether the vector length requires AVX-512VL.
    /// Both forms specify WIG and require AVX-512BW. Memory, EVEX.b, reserved
    /// vector lengths, and malformed masks fail closed.
    pub fn evex_register_packed_average_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 1
            || p1 & 0x04 == 0
            || p1 & 0x03 != 1
            || !matches!(opcode, 0xE0 | 0xE3)
            || modrm >> 6 != 3
        {
            return None;
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only EVEX packed integer bit tests that write an
    /// opmask destination and return whether the vector length requires
    /// AVX-512VL. Byte/word forms use AVX-512BW and doubleword/quadword forms
    /// use AVX-512F. The destination is restricted to canonical K0-K7
    /// encoding; EVEX.z is reserved because inactive destination bits are
    /// unconditionally zeroed by these instructions.
    pub fn evex_register_packed_test_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        let pp = p1 & 0x03;
        if p0 & 0x0f != 2
            || p0 & 0x90 != 0x90
            || p1 & 0x04 == 0
            || !matches!(pp, 1 | 2)
            || !matches!(opcode, 0x26 | 0x27)
            || modrm >> 6 != 3
        {
            return None;
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        if zeroing || embedded_control {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only EVEX signed/unsigned packed integer compares
    /// that write an opmask destination and return whether the vector length
    /// requires AVX-512VL. Byte/word forms use AVX-512BW and
    /// doubleword/quadword forms use AVX-512F. The destination is restricted
    /// to canonical K0-K7 encoding, and EVEX.z is reserved because masked-off
    /// comparison-result bits are unconditionally zeroed.
    pub fn evex_register_packed_compare_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 7 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 3
            || p0 & 0x90 != 0x90
            || p1 & 0x04 == 0
            || p1 & 0x03 != 1
            || !matches!(opcode, 0x1E | 0x1F | 0x3E | 0x3F)
            || modrm >> 6 != 3
        {
            return None;
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        if zeroing || embedded_control {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only EVEX binary32/binary64 shuffle and unpack
    /// operations and return whether the vector length requires AVX-512VL.
    /// VSHUF* carries an imm8 while VUNPCKL*/VUNPCKH* does not. Memory,
    /// broadcast, EVEX.b, reserved vector lengths, and malformed masks fail
    /// closed.
    pub fn evex_register_fp_shuffle_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if !matches!(bytes.len(), 6 | 7) || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 1 || p1 & 0x04 == 0 || modrm >> 6 != 3 {
            return None;
        }
        match opcode {
            0x14 | 0x15 if bytes.len() == 6 => {}
            0xC6 if bytes.len() == 7 => {}
            _ => return None,
        }

        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        if !matches!(pp, 0 | 1) || w != (pp == 1) {
            return None;
        }
        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only AVX-512F full-vector and in-lane dword/qword
    /// permutes. This covers VPERMD/Q/PS/PD, VPERMI2D/Q/PS/PD,
    /// VPERMT2D/Q/PS/PD, and the variable/immediate VPERMILPS/PD forms.
    /// VPERMD/Q/PS/PD exclude 128-bit vector length; the remaining forms allow
    /// 128/256/512-bit vectors. Immediate-control encodings additionally
    /// require reserved EVEX.vvvv=1111b and EVEX.V'=1. Memory/broadcast forms,
    /// EVEX.b, reserved vector lengths, and malformed masks fail closed.
    pub fn evex_register_avx512f_permute_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if !matches!(bytes.len(), 6 | 7) || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        // Every admitted form uses mandatory 66 and a register ModR/M source.
        if p1 & 0x07 != 0x05 || modrm >> 6 != 3 {
            return None;
        }
        let map = p0 & 0x0F;
        let w = p1 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let immediate_control = match (bytes.len(), map, opcode, w) {
            // Variable-control VPERMPS/PD and VPERMD/Q. EVEX.128 is reserved.
            (6, 2, 0x16 | 0x36, _) if matches!(ll, 1 | 2) => false,
            // Two-table full permutes, with W selecting D/PS or Q/PD.
            (6, 2, 0x76 | 0x77 | 0x7E | 0x7F, _) if ll <= 2 => false,
            // Variable-control in-lane permutes.
            (6, 2, 0x0C, false) | (6, 2, 0x0D, true) if ll <= 2 => false,
            // Immediate-control VPERMQ/PD. EVEX.128 is reserved.
            (7, 3, 0x00 | 0x01, true) if matches!(ll, 1 | 2) => true,
            // Immediate-control in-lane permutes.
            (7, 3, 0x04, false) | (7, 3, 0x05, true) if ll <= 2 => true,
            _ => return None,
        };

        let zeroing = p2 & 0x80 != 0;
        let embedded_broadcast = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_broadcast || (zeroing && mask == 0) {
            return None;
        }
        if immediate_control && (p1 & 0x78 != 0x78 || p2 & 0x08 == 0) {
            return None;
        }
        Some(ll != 2)
    }

    /// Validate register-source EVEX broadcasts whose repeated element or
    /// tuple has 32-bit or 64-bit granularity. The admitted encodings are
    /// VBROADCASTSS, VBROADCASTSD, VBROADCASTF32X2, VPBROADCASTD,
    /// VPBROADCASTQ, and VBROADCASTI32X2. Memory sources are excluded because
    /// native replay must not bypass guest-memory translation or writemask
    /// fault suppression. Returns `(needs AVX-512VL, needs AVX-512DQ)`.
    pub fn evex_register_broadcast_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        // Every admitted form uses map 0F38, prefix 66, reserved EVEX.vvvv=1111
        // and EVEX.V'=1, and a register ModR/M source.
        if p0 & 0x0f != 2
            || p1 & 0x04 == 0
            || p1 & 0x03 != 1
            || p1 & 0x78 != 0x78
            || p2 & 0x08 == 0
            || modrm >> 6 != 3
        {
            return None;
        }

        let w = p1 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let needs_avx512dq = match (opcode, w, ll) {
            // VBROADCASTSS and VPBROADCASTD.
            (0x18 | 0x58, false, 0..=2) => false,
            // VBROADCASTSD and VPBROADCASTQ. VBROADCASTSD excludes VL=128.
            (0x19, true, 1 | 2) | (0x59, true, 0..=2) => false,
            // VBROADCASTF32X2 excludes VL=128; VBROADCASTI32X2 permits it.
            (0x19, false, 1 | 2) | (0x59, false, 0..=2) => true,
            _ => return None,
        };
        let zeroing = p2 & 0x80 != 0;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }

        Some((ll != 2, needs_avx512dq))
    }

    /// Validate register-source EVEX VPBROADCASTB/VPBROADCASTW. These forms
    /// require AVX-512BW, while 128-bit and 256-bit destinations additionally
    /// require AVX-512VL. Memory sources are excluded from native replay so
    /// guest-memory translation and masked fault suppression remain explicit.
    pub fn evex_register_narrow_broadcast_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 2
            || p1 & 0x80 != 0
            || p1 & 0x04 == 0
            || p1 & 0x03 != 1
            || p1 & 0x78 != 0x78
            || p2 & 0x08 == 0
            || !matches!(opcode, 0x78 | 0x79)
            || modrm >> 6 != 3
        {
            return None;
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }
}

/// A contiguous semantic-op group that may be replaced by one exact native x86
/// instruction after byte-level validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct X86NativeReplaySpan {
    /// Exclusive semantic-op end index.
    pub end: usize,
    /// Exact source instruction to emit.
    pub instruction: X86InstructionBytes,
    /// Whether native execution requires AVX-512VL.
    pub needs_avx512vl: bool,
    /// Whether native execution requires AVX-512DQ.
    pub needs_avx512dq: bool,
    /// Whether native execution requires AVX-512-FP16.
    pub needs_avx512fp16: bool,
}

/// Compatibility name for the first replay family.
pub type X86EvexFpReplaySpan = X86NativeReplaySpan;

fn x86_evex_replay_spans_where(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    classify: impl Fn(&X86InstructionBytes) -> Option<(bool, bool, bool)>,
) -> HashMap<usize, X86NativeReplaySpan> {
    let mut groups = HashMap::<GuestAddr, (usize, usize, bool)>::new();
    for (index, op) in block.ops.iter().enumerate() {
        groups
            .entry(op.guest_pc)
            .and_modify(|(_, end, contiguous)| {
                if *end != index {
                    *contiguous = false;
                }
                *end = index + 1;
            })
            .or_insert((index, index + 1, true));
    }

    groups
        .into_iter()
        .filter_map(|(guest_pc, (start, end, contiguous))| {
            if !contiguous {
                return None;
            }
            let instruction = *instruction_bytes.get(&(block.id, guest_pc))?;
            let (needs_avx512vl, needs_avx512dq, needs_avx512fp16) = classify(&instruction)?;
            Some((
                start,
                X86NativeReplaySpan {
                    end,
                    instruction,
                    needs_avx512vl,
                    needs_avx512dq,
                    needs_avx512fp16,
                },
            ))
        })
        .collect()
}

/// Identify valid register-only EVEX floating-point replay groups in `block`.
/// Construction is O(N) time and O(P) space for N SMIR operations and P unique
/// guest PCs. A guest PC occurring in multiple non-contiguous groups is
/// rejected, preventing one source instruction from replacing reordered or
/// fabricated semantic fragments.
pub fn x86_evex_fp_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86EvexFpReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_fp_arithmetic_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX logical replay groups in `block` in O(N)
/// time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_logic_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_logic_requirements()
            .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
    })
}

/// Identify valid register-only EVEX packed integer arithmetic replay groups
/// in `block` in O(N) time and O(P) space for N operations and P unique guest
/// PCs.
pub fn x86_evex_integer_arithmetic_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_integer_arithmetic_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX shared-count shift replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_shared_count_shift_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_shared_count_shift_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX immediate-count shift replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_immediate_count_shift_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_immediate_count_shift_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX packed FMA replay groups in `block` in
/// O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_packed_fma_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_packed_fma_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX scalar FMA replay groups in `block` in
/// O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_scalar_fma_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_scalar_fma_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX packed binary16 FMA replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_packed_fp16_fma_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_packed_fp16_fma_needs_vl()
            .map(|needs_vl| (needs_vl, false, true))
    })
}

/// Identify valid register-only EVEX scalar binary16 FMA replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_scalar_fp16_fma_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_scalar_fp16_fma_needs_vl()
            .map(|needs_vl| (needs_vl, false, true))
    })
}

/// Identify valid register-only EVEX scalar binary16 arithmetic and
/// square-root replay groups in `block` in O(N) time and O(P) space for N
/// operations and P unique guest PCs.
pub fn x86_evex_scalar_fp16_arithmetic_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_scalar_fp16_arithmetic_needs_vl()
            .map(|needs_vl| (needs_vl, false, true))
    })
}

/// Identify valid register-only EVEX packed integer min/max replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_integer_minmax_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_integer_minmax_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX packed integer multiply replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_integer_multiply_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_integer_multiply_requirements()
            .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
    })
}

/// Identify valid register-only EVEX packed integer interleave replay groups
/// in `block` in O(N) time and O(P) space for N operations and P unique guest
/// PCs.
pub fn x86_evex_integer_interleave_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_integer_interleave_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX signed/unsigned saturating pack replay
/// groups in `block` in O(N) time and O(P) space for N operations and P unique
/// guest PCs.
pub fn x86_evex_integer_pack_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_integer_pack_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX packed integer absolute-value replay
/// groups in `block` in O(N) time and O(P) space for N operations and P unique
/// guest PCs.
pub fn x86_evex_packed_abs_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_packed_abs_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX rounded unsigned packed average replay
/// groups in `block` in O(N) time and O(P) space for N operations and P unique
/// guest PCs.
pub fn x86_evex_packed_average_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_packed_average_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX packed integer test replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_packed_test_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_packed_test_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX packed integer compare replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_packed_compare_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_packed_compare_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only EVEX floating shuffle/interleave replay groups
/// in `block` in O(N) time and O(P) space for N operations and P unique guest
/// PCs.
pub fn x86_evex_fp_shuffle_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_fp_shuffle_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-only AVX-512F dword/qword full-vector and in-lane
/// permute replay groups in `block` in O(N) time and O(P) space for N
/// operations and P unique guest PCs.
pub fn x86_evex_avx512f_permute_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_avx512f_permute_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify valid register-source EVEX 32/64-bit broadcast replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_broadcast_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_broadcast_requirements()
            .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
    })
}

/// Identify valid register-source EVEX byte/word broadcast replay groups in
/// `block` in O(N) time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_narrow_broadcast_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_narrow_broadcast_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
    })
}

/// Identify every validated native EVEX replay group in one O(N)-time,
/// O(P)-space block pass. Classifiers are intentionally disjoint and ordered
/// explicitly so adding a replay family does not add another scan of the SMIR
/// operation stream.
pub fn x86_evex_native_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        if let Some(needs_vl) = instruction.evex_register_fp_arithmetic_needs_vl() {
            return Some((needs_vl, false, false));
        }
        if let Some(requirements) = instruction.evex_register_logic_requirements() {
            return Some((requirements.0, requirements.1, false));
        }
        instruction
            .evex_register_integer_arithmetic_needs_vl()
            .map(|needs_vl| (needs_vl, false, false))
            .or_else(|| {
                instruction
                    .evex_register_shared_count_shift_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_immediate_count_shift_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_fma_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_scalar_fma_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_fp16_fma_needs_vl()
                    .map(|needs_vl| (needs_vl, false, true))
            })
            .or_else(|| {
                instruction
                    .evex_register_scalar_fp16_fma_needs_vl()
                    .map(|needs_vl| (needs_vl, false, true))
            })
            .or_else(|| {
                instruction
                    .evex_register_scalar_fp16_arithmetic_needs_vl()
                    .map(|needs_vl| (needs_vl, false, true))
            })
            .or_else(|| {
                instruction
                    .evex_register_integer_minmax_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_integer_multiply_requirements()
                    .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_integer_interleave_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_integer_pack_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_abs_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_average_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_test_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_packed_compare_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_fp_shuffle_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_avx512f_permute_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_broadcast_requirements()
                    .map(|(needs_vl, needs_dq)| (needs_vl, needs_dq, false))
            })
            .or_else(|| {
                instruction
                    .evex_register_narrow_broadcast_needs_vl()
                    .map(|needs_vl| (needs_vl, false, false))
            })
    })
}

#[cfg(test)]
#[path = "x86_native_replay_tests.rs"]
mod tests;
