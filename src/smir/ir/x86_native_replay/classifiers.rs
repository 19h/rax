use super::X86InstructionBytes;

mod chunk;
mod evex_fma3_memory;
mod fp16_narrow;
mod fp16_widen;
mod fp32_fp64_convert;
mod fp_arithmetic;
mod fp_class;
mod fp_compare;
mod fp_estimate;
mod fp_horizontal;
mod fp_round;
mod fp_shuffle;
mod fp_sqrt;
mod gfni;
mod high_low_move;
mod integer_compare;
mod packed_extend;
mod packed_move;
mod scalar_fp_convert;
mod scalar_fp_to_int;
mod scalar_int_to_fp;
mod scalar_integer_move;
mod scalar_lane_transfer;
mod scalar_move;
mod vex_alignr;
mod vex_chunk_extract;
mod vex_cross_lane_128;
mod vex_fma3;
mod vex_fma4;
mod vex_fp_dot_product;
mod vex_fp_logic;
mod vex_horizontal_integer;
mod vex_immediate_blend;
mod vex_immediate_permute;
mod vex_integer_dot_ext;
mod vex_lane_shuffle;
mod vex_memory;
mod vex_mov_mask;
mod vex_pabs;
mod vex_packed_string;
mod vex_pavg;
mod vex_pmulhrsw;
mod vex_psign;
mod vex_ptest;
mod vex_register_broadcast;
mod vex_scalar_extract;
mod vex_scalar_insert;
mod vex_scalar_vmovq;
mod vex_variable_blend;
mod vex_variable_permute;
mod vex_vpermil2;
mod vex_widening_dword_multiply;
mod vex_zero;
mod vp2intersect;
mod vpclmulqdq;

pub(crate) use evex_fma3_memory::{
    X86EvexPackedFma3MemoryEncoding, X86EvexPackedFma3MemoryReplay, X86EvexScalarFma3MemoryEncoding,
};

impl X86InstructionBytes {
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
    /// VSQRTSH require AVX-512-FP16 but not AVX-512VL. Without embedded
    /// rounding their L'L field is LLIG and accepts the three defined EVEX
    /// vector-length encodings. Register-source EVEX.b supplies embedded
    /// rounding for arithmetic/square-root or SAE for minimum/maximum; only
    /// embedded rounding repurposes L'L and makes all four values valid.
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
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        let has_embedded_rounding = !matches!(opcode, 0x5D | 0x5F);
        if (zeroing && mask == 0) || (ll == 3 && !(embedded_control && has_embedded_rounding)) {
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

    /// Validate register-only EVEX fixed-predicate and immediate-predicate
    /// signed/unsigned packed integer compares that write an opmask
    /// destination, and return whether the vector length requires AVX-512VL.
    /// Byte/word forms use AVX-512BW and doubleword/quadword forms use
    /// AVX-512F. The destination is restricted to canonical K0-K7 encoding,
    /// fixed-W and WIG forms are distinguished exactly, and EVEX.z is reserved
    /// because masked-off comparison-result bits are unconditionally zeroed.
    pub fn evex_register_packed_compare_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if !matches!(bytes.len(), 6 | 7) || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x90 != 0x90 || p1 & 0x04 == 0 || p1 & 0x03 != 1 || modrm >> 6 != 3 {
            return None;
        }

        let map = p0 & 0x0f;
        let w = p1 & 0x80 != 0;
        match (bytes.len(), map, opcode, w) {
            // VPCMP[U]D/Q/B/W imm8 families.
            (7, 3, 0x1E | 0x1F | 0x3E | 0x3F, _) => {}
            // VPCMPEQB/W and VPCMPGTB/W are WIG.
            (6, 1, 0x64 | 0x65 | 0x74 | 0x75, _) => {}
            // VPCMPEQD and VPCMPGTD require W0.
            (6, 1, 0x66 | 0x76, false) => {}
            // VPCMPEQQ and VPCMPGTQ require W1.
            (6, 2, 0x29 | 0x37, true) => {}
            _ => return None,
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

    /// Validate register-only EVEX opmask-selector blends and return whether
    /// the vector length requires AVX-512VL. The selector may be k0 (no
    /// control mask), but EVEX.z then remains reserved. EVEX.b is reserved for
    /// every register source.
    pub fn evex_register_mask_blend_needs_vl(&self) -> Option<bool> {
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
            || p1 & 0x04 == 0
            || p1 & 0x03 != 1
            || !matches!(opcode, 0x64..=0x66)
            || modrm >> 6 != 3
        {
            return None;
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let broadcast = p2 & 0x10 != 0;
        let selector = p2 & 0x07;
        if broadcast || ll == 3 || (zeroing && selector == 0) {
            return None;
        }
        Some(ll != 2)
    }

    /// Validate register-only EVEX VPMOVB2M/W2M/D2M/Q2M and return
    /// `(needs AVX-512VL, needs AVX-512DQ)`. The byte/word forms require
    /// AVX-512BW; the native vector-state trampoline already requires that
    /// feature for full-width opmask marshalling. Every E7NM reserved field,
    /// including both K-destination extension bits, is checked exactly.
    pub fn evex_register_vector_to_mask_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        // Map 2 (0F38), canonical K0-K7 destination encoding, F3, fixed-one
        // P1 bit, reserved EVEX.vvvv=1111b, and a register-only source.
        if p0 & 0x9F != 0x92
            || p1 & 0x7F != 0x7E
            || !matches!(opcode, 0x29 | 0x39)
            || modrm >> 6 != 3
        {
            return None;
        }

        let ll = (p2 >> 5) & 0x03;
        // EVEX.z/b/aaa are reserved and V' must retain its encoded-one value.
        if p2 & 0x9F != 0x08 || ll == 3 {
            return None;
        }

        Some((ll != 2, opcode == 0x39))
    }

    /// Validate register-only EVEX VPMOVM2B/W/D/Q and return
    /// `(needs AVX-512VL, needs AVX-512DQ)`. The byte/word forms require
    /// AVX-512BW; the native vector-state trampoline already requires that
    /// feature for full-width opmask marshalling. EVEX.X/B are accepted in
    /// either state because Intel SDM Table 2-41 defines them as ignored for a
    /// ModR/M.r/m K-register operand; EVEX.R/R' select the vector destination.
    pub fn evex_register_mask_to_vector_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        // Map 2 (0F38), F3, fixed-one P1 bit, reserved EVEX.vvvv=1111b,
        // and a register-only K source. All four vector-destination extension
        // buckets are valid.
        if p0 & 0x0F != 0x02
            || p1 & 0x7F != 0x7E
            || !matches!(opcode, 0x28 | 0x38)
            || modrm >> 6 != 3
        {
            return None;
        }

        let ll = (p2 >> 5) & 0x03;
        // EVEX.z/b/aaa are reserved and V' must retain its encoded-one value.
        if p2 & 0x9F != 0x08 || ll == 3 {
            return None;
        }

        Some((ll != 2, opcode == 0x38))
    }

    /// Validate register-only EVEX VPBROADCASTMB2Q/MW2D and return whether
    /// the vector length requires AVX-512VL. Both forms require AVX-512CD.
    /// EVEX.X/B are accepted in either state because Intel SDM Table 2-41
    /// defines them as ignored for a ModR/M.r/m K-register operand; EVEX.R/R'
    /// select the vector destination. Every Type E6NF reserved field and
    /// memory form fails closed.
    pub fn evex_register_mask_broadcast_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        // Map 2 (0F38), F3, fixed-one P1 bit, reserved EVEX.vvvv=1111b,
        // and a register-only K source. All vector-destination extension
        // buckets and all architecturally ignored X/B encodings are valid.
        if p0 & 0x0F != 0x02
            || p1 & 0x7F != 0x7E
            || !matches!((opcode, p1 & 0x80 != 0), (0x2A, true) | (0x3A, false))
            || modrm >> 6 != 3
        {
            return None;
        }

        let ll = (p2 >> 5) & 0x03;
        // EVEX.z/b/aaa are reserved and V' must retain its encoded-one value.
        if p2 & 0x9F != 0x08 || ll == 3 {
            return None;
        }

        Some(ll != 2)
    }

    /// Validate register-only EVEX one-source lane shuffles and return whether
    /// the vector length requires AVX-512VL. This covers VMOVSLDUP,
    /// VMOVSHDUP, VMOVDDUP, VPSHUFD, VPSHUFHW, and VPSHUFLW. The word
    /// shuffles are WIG; the other four forms have fixed W values. Memory
    /// sources, embedded broadcast, reserved EVEX.vvvv/V', malformed masks,
    /// reserved vector lengths, and incorrect instruction lengths fail closed.
    pub fn evex_register_lane_shuffle_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if !matches!(bytes.len(), 6 | 7) || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        // Every admitted form uses map 0F, reserved EVEX.vvvv=1111b and
        // EVEX.V'=1, and a register ModR/M source.
        if p0 & 0x0F != 1
            || p1 & 0x04 == 0
            || p1 & 0x78 != 0x78
            || p2 & 0x08 == 0
            || modrm >> 6 != 3
        {
            return None;
        }

        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        match (bytes.len(), opcode, pp, w) {
            // Fixed-W duplicate moves.
            (6, 0x12, 2, false) | (6, 0x16, 2, false) | (6, 0x12, 3, true) => {}
            // VPSHUFD is fixed W0; VPSHUFHW/LW are WIG.
            (7, 0x70, 1, false) | (7, 0x70, 2 | 3, _) => {}
            _ => return None,
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_broadcast = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_broadcast || ll == 3 || (zeroing && mask == 0) {
            return None;
        }
        Some(ll != 2)
    }

    /// Validate register-only EVEX VALIGND/Q and return whether the vector
    /// length requires AVX-512VL. All vector register-extension channels and
    /// every imm8 value are architectural. Memory sources, EVEX.b, reserved
    /// vector lengths, malformed masks, and incorrect lengths fail closed.
    pub fn evex_register_vector_align_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 7 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p0 & 0x0F != 3 || p1 & 0x04 == 0 || p1 & 0x03 != 1 || opcode != 0x03 || modrm >> 6 != 3 {
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
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only EVEX VPSHUFB/VPMADDUBSW/VPMADDWD, returning
    /// whether AVX-512VL is required. EVEX.W is ignored for these AVX-512BW
    /// operations. Memory, EVEX.b, reserved vector lengths, malformed masks,
    /// incorrect prefixes/opcodes, and incorrect lengths fail closed.
    pub fn evex_register_bw_shuffle_madd_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        let map_opcode = (p0 & 0x0F, opcode);

        if p1 & 0x04 == 0
            || p1 & 0x03 != 1
            || !matches!(map_opcode, (2, 0x00 | 0x04) | (1, 0xF5))
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
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate register-only EVEX VPALIGNR/VDBPSADBW and return whether the
    /// vector length requires AVX-512VL. VPALIGNR is WIG; VDBPSADBW requires
    /// W0. Every imm8 is architectural. Memory, EVEX.b, reserved vector
    /// lengths, malformed masks, incorrect prefixes/opcodes, and incorrect
    /// lengths fail closed.
    pub fn evex_register_bw_immediate_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 7 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        let w = p1 & 0x80 != 0;

        if p0 & 0x0F != 3
            || p1 & 0x04 == 0
            || p1 & 0x03 != 1
            || !matches!((opcode, w), (0x0F, _) | (0x42, false))
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

    /// Validate register-only EVEX packed moves and return whether the vector
    /// length requires AVX-512VL. This covers VMOVUPS/UPD, VMOVAPS/APD,
    /// VMOVDQA32/64, and VMOVDQU8/16/32/64 in both opcode directions. Exact
    /// mandatory-prefix and W combinations distinguish all ten mnemonics.
    /// Reserved EVEX.vvvv/V', EVEX.b, vector length, masking, and memory forms
    /// fail closed; aligned forms are safe because no memory operand is
    /// admitted.
    pub fn evex_register_packed_move_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        // Every admitted form uses map 0F, reserved EVEX.vvvv=1111b and
        // EVEX.V'=1, and a register ModR/M operand.
        if p0 & 0x0f != 1
            || p1 & 0x04 == 0
            || p1 & 0x78 != 0x78
            || p2 & 0x08 == 0
            || modrm >> 6 != 3
        {
            return None;
        }

        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        match (opcode, pp, w) {
            // VMOVUPS/APS and VMOVUPD/APD, load and store opcode directions.
            (0x10 | 0x11 | 0x28 | 0x29, 0, false)
            | (0x10 | 0x11 | 0x28 | 0x29, 1, true)
            // VMOVDQA32/64 and VMOVDQU8/16/32/64, both directions.
            | (0x6f | 0x7f, 1..=3, _) => {}
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

    /// Validate EVEX VPBROADCASTB/W/D/Q forms whose source is a GPR. The
    /// identity-map trampoline can replay every GPR source except RSP and RBP,
    /// which hold the host stack/frame state inside generated code. EVEX.X is
    /// ignored for a GPR ModR/M operand; EVEX.B selects GPRs 8 through 15.
    /// Returns whether the vector length additionally requires AVX-512VL.
    pub fn evex_register_gpr_broadcast_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p0 & 0x0F != 2
            || p1 & 0x04 == 0
            || p1 & 0x03 != 1
            || p1 & 0x78 != 0x78
            || p2 & 0x08 == 0
            || modrm >> 6 != 3
        {
            return None;
        }

        let w = p1 & 0x80 != 0;
        if !matches!((opcode, w), (0x7A | 0x7B | 0x7C, false) | (0x7C, true)) {
            return None;
        }
        let source_low = modrm & 0x07;
        let source_is_low_gpr = p0 & 0x20 != 0;
        if source_is_low_gpr && matches!(source_low, 4 | 5) {
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
