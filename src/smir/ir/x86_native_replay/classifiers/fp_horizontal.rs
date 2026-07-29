//! x86 packed floating-point horizontal/add-sub replay classification.

use super::X86InstructionBytes;
use crate::smir::ir::types::{VecElementType, VecWidth, X86FpBinaryOp};

const OPCODES: [u8; 3] = [0x7C, 0x7D, 0xD0];

impl X86InstructionBytes {
    /// Validate one complete AVX VEX packed `HADD`/`HSUB`/`ADDSUB`
    /// binary32/binary64 instruction whose second source is memory and return
    /// `(destination, first source, element type, operation, width, opcode,
    /// W)`.
    ///
    /// These forms use map 0F, mandatory prefix F2 for binary32 or 66 for
    /// binary64, and admit 128-bit and 256-bit vector lengths. VEX.W is
    /// ignored by the architecture and retained here so callers can prove
    /// that the SMIR hint and source bytes agree before canonicalizing native
    /// replay to W=0. The shared parser validates the complete
    /// ModR/M/SIB/displacement byte shape and permits only
    /// segment/address-size legacy prefixes.
    pub(crate) fn vex_memory_fp_horizontal_addsub_fields(
        &self,
    ) -> Option<(u8, u8, VecElementType, X86FpBinaryOp, VecWidth, u8, bool)> {
        let fields = self.vex_memory_fields()?;
        if fields.map != 1 || !matches!(fields.pp, 1 | 3) {
            return None;
        }
        let operation = match fields.opcode {
            0x7C => X86FpBinaryOp::HorizontalAdd,
            0x7D => X86FpBinaryOp::HorizontalSub,
            0xD0 => X86FpBinaryOp::AddSub,
            _ => return None,
        };
        Some((
            fields.destination,
            fields.source1,
            if fields.pp == 3 {
                VecElementType::F32
            } else {
                VecElementType::F64
            },
            operation,
            if fields.width_256 {
                VecWidth::V256
            } else {
                VecWidth::V128
            },
            fields.opcode,
            fields.w,
        ))
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [(u8, X86FpBinaryOp); 3] = [
        (0x7C, X86FpBinaryOp::HorizontalAdd),
        (0x7D, X86FpBinaryOp::HorizontalSub),
        (0xD0, X86FpBinaryOp::AddSub),
    ];

    fn instruction(
        destination: u8,
        source1: u8,
        base: u8,
        opcode: u8,
        elem: VecElementType,
        width: VecWidth,
        w: bool,
    ) -> Vec<u8> {
        let l = u8::from(width == VecWidth::V256);
        vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 1,
            (u8::from(w) << 7)
                | (((!source1) & 0x0F) << 3)
                | (l << 2)
                | if elem == VecElementType::F32 { 3 } else { 1 },
            opcode,
            0x40 | ((destination & 7) << 3) | (base & 7),
            0x20,
        ]
    }

    #[test]
    fn classifies_every_destination_source_kind_format_width_and_w_cell() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for source1 in 0..16 {
                for (opcode, operation) in KINDS {
                    for elem in [VecElementType::F32, VecElementType::F64] {
                        for width in [VecWidth::V128, VecWidth::V256] {
                            for base in [3, 11] {
                                for w in [false, true] {
                                    let bytes = instruction(
                                        destination,
                                        source1,
                                        base,
                                        opcode,
                                        elem,
                                        width,
                                        w,
                                    );
                                    let metadata = X86InstructionBytes::new(&bytes).unwrap();
                                    assert_eq!(
                                        metadata.vex_memory_fp_horizontal_addsub_fields(),
                                        Some((
                                            destination,
                                            source1,
                                            elem,
                                            operation,
                                            width,
                                            opcode,
                                            w,
                                        )),
                                        "{bytes:02X?}"
                                    );
                                    classified += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 3 * 2 * 2 * 2 * 2);
    }

    #[test]
    fn accepts_c5_and_complete_prefixed_sib_displacement_shapes() {
        for (bytes, expected) in [
            (
                vec![0xC5, 0xF3, 0xD0, 0x43, 0x20],
                (
                    0,
                    1,
                    VecElementType::F32,
                    X86FpBinaryOp::AddSub,
                    VecWidth::V128,
                    0xD0,
                    false,
                ),
            ),
            (
                vec![
                    0x64, 0x67, 0xC4, 0x01, 0xB5, 0x7D, 0xB4, 0x7E, 0x11, 0x22, 0x33, 0x44,
                ],
                (
                    14,
                    9,
                    VecElementType::F64,
                    X86FpBinaryOp::HorizontalSub,
                    VecWidth::V256,
                    0x7D,
                    true,
                ),
            ),
        ] {
            let metadata = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                metadata.vex_memory_fp_horizontal_addsub_fields(),
                Some(expected),
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn malformed_or_semantically_different_memory_encodings_fail_closed() {
        let valid = instruction(3, 9, 11, 0x7C, VecElementType::F64, VecWidth::V128, false);
        let mut cases = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 2;
        cases.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        cases.push(wrong_prefix);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x7E;
        cases.push(wrong_opcode);

        let mut register_source = valid.clone();
        register_source[4] |= 0xC0;
        register_source.truncate(5);
        cases.push(register_source);

        let mut trailing = valid.clone();
        trailing.push(0);
        cases.push(trailing);

        let mut truncated = valid.clone();
        truncated.pop();
        cases.push(truncated);

        let mut forbidden_legacy_prefix = valid;
        forbidden_legacy_prefix.insert(0, 0x66);
        cases.push(forbidden_legacy_prefix);

        for bytes in cases {
            let metadata = X86InstructionBytes::new(&bytes).unwrap();
            assert_eq!(
                metadata.vex_memory_fp_horizontal_addsub_fields(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
