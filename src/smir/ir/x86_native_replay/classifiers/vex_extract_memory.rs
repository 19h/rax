//! AVX VEX scalar and 128-bit chunk memory-destination extraction.

use super::X86InstructionBytes;
use crate::smir::ir::types::{MemWidth, VecElementType};

/// One complete VEX.128 scalar-extract memory encoding rewritten to target
/// preserved host RAX.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexScalarExtractMemoryEncoding {
    pub(crate) source: u8,
    pub(crate) lane: u8,
    pub(crate) elem: VecElementType,
    pub(crate) memory_width: MemWidth,
    pub(crate) w: bool,
    pub(crate) opcode: u8,
    pub(crate) immediate: u8,
    pub(crate) register_instruction: X86InstructionBytes,
}

/// One complete VEX.256 128-bit-chunk extraction to memory rewritten to
/// target a borrowed low XMM register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexChunkExtractMemoryEncoding {
    pub(crate) source: u8,
    pub(crate) first_lane: u8,
    pub(crate) scratch: u8,
    pub(crate) needs_avx2: bool,
    pub(crate) opcode: u8,
    pub(crate) immediate: u8,
    pub(crate) register_instruction: X86InstructionBytes,
}

impl X86InstructionBytes {
    /// Rewrite only the ModR/M memory destination of one complete VEX
    /// instruction carrying an imm8, preserving that immediate exactly.
    fn vex_memory_with_register_destination_and_imm8(&self, destination: u8) -> Option<Self> {
        let (immediate, instruction) = self.as_slice().split_last()?;
        let instruction = Self::new(instruction)?;
        let rewritten = instruction.vex_memory_with_register_source(destination)?;
        let mut bytes = [0u8; 15];
        let len = rewritten.as_slice().len();
        if len == bytes.len() {
            return None;
        }
        bytes[..len].copy_from_slice(rewritten.as_slice());
        bytes[len] = *immediate;
        Self::new(&bytes[..=len])
    }

    /// Validate and rewrite one defined AVX VEX scalar extraction whose
    /// destination is memory.
    ///
    /// The admitted family is `VPEXTRB`, map-0F3A `VPEXTRW`,
    /// `VPEXTRD/Q`, and `VEXTRACTPS`. Every form is VEX.128.66.0F3A,
    /// reserves VEX.vvvv=`1111b`, and uses an unaligned 1-/2-/4-/8-byte
    /// memory destination. W selects `VPEXTRD`/`VPEXTRQ` for opcode 16H and
    /// is ignored for the other opcodes. Immediate high bits are preserved in
    /// the rewritten instruction while the reported lane applies the
    /// architectural mask.
    pub(crate) fn vex_scalar_extract_memory_encoding(
        &self,
    ) -> Option<X86VexScalarExtractMemoryEncoding> {
        let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 3 || fields.pp != 1 || fields.width_256 || fields.source1 != 0 {
            return None;
        }
        let (elem, memory_width, lane_mask) = match (fields.opcode, fields.w) {
            (0x14, _) => (VecElementType::I8, MemWidth::B1, 0x0F),
            (0x15, _) => (VecElementType::I16, MemWidth::B2, 0x07),
            (0x16, false) | (0x17, _) => (VecElementType::I32, MemWidth::B4, 0x03),
            (0x16, true) => (VecElementType::I64, MemWidth::B8, 0x01),
            _ => return None,
        };
        let register_instruction = self.vex_memory_with_register_destination_and_imm8(0)?;
        Some(X86VexScalarExtractMemoryEncoding {
            source: fields.destination,
            lane: immediate & lane_mask,
            elem,
            memory_width,
            w: fields.w,
            opcode: fields.opcode,
            immediate,
            register_instruction,
        })
    }

    /// Validate and rewrite one defined AVX/AVX2 128-bit chunk extraction
    /// whose destination is memory.
    ///
    /// `VEXTRACTF128` and `VEXTRACTI128` are
    /// VEX.256.66.0F3A.W0, reserve VEX.vvvv=`1111b`, and use imm8 bit 0 to
    /// select one of two 128-bit source chunks. The floating form requires
    /// AVX; the integer form requires AVX2. A low XMM register distinct from
    /// the source is borrowed as the rewritten register destination.
    pub(crate) fn vex_chunk_extract_memory_encoding(
        &self,
    ) -> Option<X86VexChunkExtractMemoryEncoding> {
        let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 3 || fields.pp != 1 || !fields.width_256 || fields.w || fields.source1 != 0
        {
            return None;
        }
        let needs_avx2 = match fields.opcode {
            0x19 => false,
            0x39 => true,
            _ => return None,
        };
        let scratch = (0..8u8)
            .find(|candidate| *candidate != fields.destination)
            .expect("one VEX source leaves at least seven low scratch registers");
        let register_instruction = self.vex_memory_with_register_destination_and_imm8(scratch)?;
        Some(X86VexChunkExtractMemoryEncoding {
            source: fields.destination,
            first_lane: (immediate & 1) * 2,
            scratch,
            needs_avx2,
            opcode: fields.opcode,
            immediate,
            register_instruction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instruction(source: u8, base: u8, opcode: u8, w: bool, l: bool, immediate: u8) -> Vec<u8> {
        assert!(source < 16 && base < 16);
        vec![
            0xC4,
            (if source < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 3,
            (u8::from(w) << 7) | 0x78 | (u8::from(l) << 2) | 1,
            opcode,
            0x40 | ((source & 7) << 3) | (base & 7),
            0x20,
            immediate,
        ]
    }

    fn shared_invalid_variants(valid: &[u8]) -> Vec<Vec<u8>> {
        let mut invalid = Vec::new();
        let mut vvvv = valid.to_vec();
        vvvv[2] &= !0x08;
        invalid.push(vvvv);
        let mut pp = valid.to_vec();
        pp[2] = (pp[2] & !3) | 2;
        invalid.push(pp);
        let mut map = valid.to_vec();
        map[1] = (map[1] & !0x1F) | 2;
        invalid.push(map);
        let mut register = valid.to_vec();
        register[4] |= 0xC0;
        register.remove(5);
        invalid.push(register);
        let mut trailing = valid.to_vec();
        trailing.push(0);
        invalid.push(trailing);
        let mut forbidden_prefix = valid.to_vec();
        forbidden_prefix.insert(0, 0x66);
        invalid.push(forbidden_prefix);
        for end in 0..valid.len() {
            invalid.push(valid[..end].to_vec());
        }
        invalid
    }

    #[test]
    fn classifies_and_rewrites_every_scalar_source_form_and_immediate() {
        let forms = [
            (0x14, false, VecElementType::I8, MemWidth::B1, 0x0F),
            (0x14, true, VecElementType::I8, MemWidth::B1, 0x0F),
            (0x15, false, VecElementType::I16, MemWidth::B2, 0x07),
            (0x15, true, VecElementType::I16, MemWidth::B2, 0x07),
            (0x16, false, VecElementType::I32, MemWidth::B4, 0x03),
            (0x16, true, VecElementType::I64, MemWidth::B8, 0x01),
            (0x17, false, VecElementType::I32, MemWidth::B4, 0x03),
            (0x17, true, VecElementType::I32, MemWidth::B4, 0x03),
        ];
        let mut classified = 0usize;
        for source in 0..16 {
            for &(opcode, w, elem, memory_width, lane_mask) in &forms {
                for immediate in u8::MIN..=u8::MAX {
                    let bytes = instruction(source, 11, opcode, w, false, immediate);
                    let encoding = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .vex_scalar_extract_memory_encoding()
                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                    assert_eq!(encoding.source, source);
                    assert_eq!(encoding.lane, immediate & lane_mask);
                    assert_eq!(encoding.elem, elem);
                    assert_eq!(encoding.memory_width, memory_width);
                    assert_eq!(encoding.w, w);
                    assert_eq!(encoding.opcode, opcode);
                    assert_eq!(encoding.immediate, immediate);
                    assert!(
                        encoding
                            .register_instruction
                            .is_vex_register_scalar_extract()
                    );
                    assert_eq!(
                        encoding
                            .register_instruction
                            .vex_scalar_extract_destination_index(),
                        Some(0)
                    );
                    assert_eq!(
                        encoding.register_instruction.as_slice().last(),
                        Some(&immediate)
                    );
                    classified += 1;
                }
            }
        }
        assert_eq!(classified, 16 * 8 * 256);
    }

    #[test]
    fn classifies_and_rewrites_every_chunk_source_opcode_and_immediate() {
        let mut classified = 0usize;
        for source in 0..16 {
            for (opcode, needs_avx2) in [(0x19, false), (0x39, true)] {
                for immediate in u8::MIN..=u8::MAX {
                    let bytes = instruction(source, 11, opcode, false, true, immediate);
                    let encoding = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .vex_chunk_extract_memory_encoding()
                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                    assert_eq!(encoding.source, source);
                    assert_eq!(encoding.first_lane, (immediate & 1) * 2);
                    assert!(encoding.scratch < 8);
                    assert_ne!(encoding.scratch, source);
                    assert_eq!(encoding.needs_avx2, needs_avx2);
                    assert_eq!(encoding.opcode, opcode);
                    assert_eq!(encoding.immediate, immediate);
                    assert_eq!(
                        encoding
                            .register_instruction
                            .vex_register_chunk_extract_needs_avx2(),
                        Some(needs_avx2)
                    );
                    assert_eq!(
                        encoding
                            .register_instruction
                            .vex_chunk_extract_destination_index(),
                        Some(encoding.scratch)
                    );
                    assert_eq!(
                        encoding.register_instruction.as_slice().last(),
                        Some(&immediate)
                    );
                    classified += 1;
                }
            }
        }
        assert_eq!(classified, 16 * 2 * 256);
    }

    #[test]
    fn reserved_fields_register_destinations_and_nonexact_images_fail_closed() {
        let scalar = instruction(9, 11, 0x14, true, false, 0xA5);
        let chunk = instruction(9, 11, 0x39, false, true, 0xA5);
        let mut invalid_scalar = shared_invalid_variants(&scalar);
        let mut invalid_chunk = shared_invalid_variants(&chunk);

        let mut scalar_l1 = scalar.clone();
        scalar_l1[2] |= 0x04;
        invalid_scalar.push(scalar_l1);
        let mut chunk_l0 = chunk.clone();
        chunk_l0[2] &= !0x04;
        invalid_chunk.push(chunk_l0);
        let mut chunk_w1 = chunk.clone();
        chunk_w1[2] |= 0x80;
        invalid_chunk.push(chunk_w1);

        for bytes in invalid_scalar {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .and_then(|instruction| instruction.vex_scalar_extract_memory_encoding()),
                None,
                "{bytes:02X?}"
            );
        }
        for bytes in invalid_chunk {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .and_then(|instruction| instruction.vex_chunk_extract_memory_encoding()),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
