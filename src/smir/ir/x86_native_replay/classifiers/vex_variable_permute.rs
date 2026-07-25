//! Register-only AVX/AVX2 VEX variable permutes.

use super::X86InstructionBytes;

impl X86InstructionBytes {
    /// Validate one exact five-byte register-only VEX variable permute and
    /// report whether the selected form requires AVX2 rather than AVX.
    ///
    /// Intel SDM Volume 2 assigns the variable-control `VPERMILPS` and
    /// `VPERMILPD` forms to map 0F38, mandatory 66H, VEX.W=0, and opcodes
    /// 0CH/0DH. Both 128-bit and 256-bit forms require AVX. `VPERMPS` and
    /// `VPERMD` use the same map/prefix/W fields with opcodes 16H/36H, require
    /// VEX.L=1, and require AVX2. Memory forms remain excluded so replay cannot
    /// bypass guest translation or precise fault handling.
    pub fn vex_register_variable_permute_needs_avx2(&self) -> Option<bool> {
        let bytes = self.as_slice();
        let [0xC4, p0, p1, opcode, modrm] = bytes else {
            return None;
        };
        if p0 & 0x1F != 2 || p1 & 0x83 != 1 || modrm >> 6 != 3 {
            return None;
        }

        match opcode {
            0x0C | 0x0D => Some(false),
            0x16 | 0x36 if p1 & 0x04 != 0 => Some(true),
            _ => None,
        }
    }

    /// Architectural destination register selected by an exact register-only
    /// VEX variable permute. The ModR/M.reg field is extended by inverted
    /// VEX.R.
    pub(crate) fn vex_variable_permute_destination_index(&self) -> Option<u8> {
        self.vex_register_variable_permute_needs_avx2()?;
        let bytes = self.as_slice();
        let extension = u8::from(bytes[1] & 0x80 == 0) << 3;
        Some(extension | ((bytes[4] >> 3) & 7))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::ir::ops::{OpKind, SmirOp};
    use crate::smir::ir::types::{BlockId, OpId};
    use crate::smir::ir::{
        SmirBlock, x86_evex_native_replay_spans, x86_native_replay_spans,
        x86_vex_variable_blend_replay_spans, x86_vex_variable_permute_replay_spans,
    };
    use std::collections::HashMap;

    const OPCODES: [u8; 4] = [0x0C, 0x0D, 0x16, 0x36];

    fn encoding(
        extension_bits: u8,
        w: bool,
        encoded_vvvv: u8,
        l: bool,
        opcode: u8,
        modrm: u8,
    ) -> [u8; 5] {
        assert_eq!(extension_bits & !0xE0, 0);
        assert!(encoded_vvvv < 16);
        [
            0xC4,
            extension_bits | 2,
            (u8::from(w) << 7) | (encoded_vvvv << 3) | (u8::from(l) << 2) | 1,
            opcode,
            modrm,
        ]
    }

    fn expected_requirement(opcode: u8, w: bool, l: bool) -> Option<bool> {
        if w {
            return None;
        }
        match opcode {
            0x0C | 0x0D => Some(false),
            0x16 | 0x36 if l => Some(true),
            _ => None,
        }
    }

    #[test]
    fn classifier_exhaustively_covers_131_072_prefix_opcode_and_register_combinations() {
        let mut accepted = 0usize;
        let mut tested = 0usize;
        for opcode in OPCODES {
            for extension_bits in (0u8..8).map(|value| value << 5) {
                for w in [false, true] {
                    for encoded_vvvv in 0u8..16 {
                        for l in [false, true] {
                            for reg_rm in 0u8..=0x3F {
                                let bytes = encoding(
                                    extension_bits,
                                    w,
                                    encoded_vvvv,
                                    l,
                                    opcode,
                                    0xC0 | reg_rm,
                                );
                                let expected = expected_requirement(opcode, w, l);
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .vex_register_variable_permute_needs_avx2(),
                                    expected,
                                    "{bytes:02X?}"
                                );
                                accepted += usize::from(expected.is_some());
                                tested += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(accepted, 49_152);
        assert_eq!(tested, 131_072);

        // Independently assembled by LLVM 23.
        for (bytes, needs_avx2, destination) in [
            ([0xC4, 0xE2, 0x69, 0x0C, 0xCB], false, 1),
            ([0xC4, 0x42, 0x1D, 0x0D, 0xDD], false, 11),
            ([0xC4, 0xE2, 0x6D, 0x16, 0xCB], true, 1),
            ([0xC4, 0x42, 0x15, 0x36, 0xE6], true, 12),
        ] {
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                instruction.vex_register_variable_permute_needs_avx2(),
                Some(needs_avx2),
                "{bytes:02X?}"
            );
            assert_eq!(
                instruction.vex_variable_permute_destination_index(),
                Some(destination),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn classifier_rejects_every_structural_and_reserved_frontier() {
        let canonical = encoding(0xE0, false, 0x0D, true, 0x36, 0xCA);
        let mut invalid = vec![
            canonical[..4].to_vec(),
            canonical.iter().copied().chain([0]).collect(),
            [0xC5, canonical[1], canonical[2], canonical[3], canonical[4]].to_vec(),
            [0x62, canonical[1], canonical[2], canonical[3], canonical[4]].to_vec(),
        ];
        for (index, value) in [
            (1, (canonical[1] & !0x1F) | 1),
            (1, (canonical[1] & !0x1F) | 3),
            (1, (canonical[1] & !0x1F) | 4),
            (1, canonical[1] & !0x1F),
            (2, canonical[2] & !0x03),
            (2, (canonical[2] & !0x03) | 2),
            (2, (canonical[2] & !0x03) | 3),
            (3, 0x0B),
            (3, 0x0E),
            (3, 0x15),
            (3, 0x17),
            (3, 0x35),
            (3, 0x37),
            (4, canonical[4] & 0x3F),
            (4, (canonical[4] & 0x3F) | 0x40),
            (4, (canonical[4] & 0x3F) | 0x80),
        ] {
            let mut bytes = canonical;
            bytes[index] = value;
            invalid.push(bytes.to_vec());
        }
        let mut reserved_w = canonical;
        reserved_w[2] |= 0x80;
        invalid.push(reserved_w.to_vec());
        for opcode in [0x16, 0x36] {
            invalid.push(encoding(0xE0, false, 0x0D, false, opcode, 0xCA).to_vec());
        }

        for bytes in invalid {
            let instruction = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                instruction.vex_register_variable_permute_needs_avx2(),
                None,
                "{bytes:02X?}"
            );
            assert_eq!(
                instruction.vex_variable_permute_destination_index(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn dedicated_and_aggregate_spans_require_exact_contiguous_provenance() {
        let pc = 0xA11C;
        let instruction =
            X86InstructionBytes::new(&encoding(0x40, false, 3, true, 0x16, 0xFF)).unwrap();
        let mut block = SmirBlock::new(BlockId(47), pc);
        block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
        let provenance = HashMap::from([((block.id, pc), instruction)]);

        for spans in [
            x86_vex_variable_permute_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).expect("exact VEX variable-permute span");
            assert_eq!(span.end, 2);
            assert_eq!(span.instruction, instruction);
            assert!(!span.needs_avx512vl);
            assert!(!span.needs_avx512dq);
            assert!(!span.needs_avx512fp16);
            assert!(!span.preserve_mxcsr_de);
        }
        assert!(x86_vex_variable_blend_replay_spans(&block, &provenance).is_empty());
        assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());

        block.push_op(SmirOp::new(OpId(2), pc + 5, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(3), pc, OpKind::Nop));
        assert!(x86_native_replay_spans(&block, &provenance).is_empty());
    }
}
