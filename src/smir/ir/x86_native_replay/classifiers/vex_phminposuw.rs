//! Complete VEX `VPHMINPOSUW` memory-source classification.

use super::X86InstructionBytes;

/// One complete VEX.128 `VPHMINPOSUW` memory encoding rewritten to use a
/// borrowed low XMM register as its source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexPhminposuwMemoryEncoding {
    pub(crate) destination: u8,
    pub(crate) scratch: u8,
    pub(crate) w: bool,
    pub(crate) register_instruction: X86InstructionBytes,
}

impl X86InstructionBytes {
    /// Validate and rewrite one complete `VEX.128.66.0F38.WIG 41 /r`
    /// `VPHMINPOSUW` instruction whose source is memory.
    ///
    /// VEX.vvvv is reserved as encoded `1111b`, VEX.L must be zero, and W is
    /// architecturally ignored but retained in the register rewrite. The
    /// shared parser validates the complete ModR/M/SIB/displacement shape and
    /// accepts only segment/address-size legacy prefixes. The borrowed source
    /// is always distinct from the architectural destination.
    ///
    /// Runtime and auxiliary space are O(1) because architectural x86
    /// instructions are bounded to 15 bytes.
    pub(crate) fn vex_phminposuw_memory_encoding(&self) -> Option<X86VexPhminposuwMemoryEncoding> {
        let fields = self.vex_memory_fields()?;
        if fields.source1 != 0
            || fields.map != 2
            || fields.pp != 1
            || fields.opcode != 0x41
            || fields.width_256
        {
            return None;
        }
        let scratch = (0..16u8)
            .find(|candidate| *candidate != fields.destination)
            .expect("one VEX destination leaves fifteen low scratch registers");
        let register_instruction = self.vex_memory_with_register_source(scratch)?;
        Some(X86VexPhminposuwMemoryEncoding {
            destination: fields.destination,
            scratch,
            w: fields.w,
            register_instruction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instruction(destination: u8, base: u8, w: bool) -> Vec<u8> {
        assert!(destination < 16 && base < 16);
        let mut bytes = vec![
            0xC4,
            (if destination < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 2,
            (u8::from(w) << 7) | 0x78 | 1,
            0x41,
            0x40 | ((destination & 7) << 3) | (base & 7),
        ];
        if base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(0x20);
        bytes
    }

    #[test]
    fn classifies_and_rewrites_every_destination_base_extension_and_wig_cell() {
        let mut classified = 0usize;
        for destination in 0..16 {
            for base in 0..16 {
                for w in [false, true] {
                    let bytes = instruction(destination, base, w);
                    let encoding = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .vex_phminposuw_memory_encoding()
                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                    assert_eq!(encoding.destination, destination);
                    assert_eq!(encoding.scratch, if destination == 0 { 1 } else { 0 });
                    assert_ne!(encoding.scratch, destination);
                    assert_eq!(encoding.w, w);
                    assert_eq!(
                        encoding.register_instruction.as_slice(),
                        &[
                            0xC4,
                            (if destination < 8 { 0x80 } else { 0 })
                                | 0x40
                                | (if encoding.scratch < 8 { 0x20 } else { 0 })
                                | 2,
                            (u8::from(w) << 7) | 0x78 | 1,
                            0x41,
                            0xC0 | ((destination & 7) << 3) | (encoding.scratch & 7),
                        ]
                    );
                    classified += 1;
                }
            }
        }
        assert_eq!(classified, 16 * 16 * 2);
    }

    #[test]
    fn llvm_23_and_complete_prefixed_address_shapes_rewrite_exactly() {
        for (bytes, expected_destination, expected_register) in [
            (
                &[0xC4, 0x42, 0x79, 0x41, 0x4B, 0x20][..],
                9,
                &[0xC4, 0x62, 0x79, 0x41, 0xC8][..],
            ),
            (
                &[
                    0x64, 0x67, 0xC4, 0x02, 0xF9, 0x41, 0xB4, 0x7E, 0x44, 0x33, 0x22, 0x11,
                ][..],
                14,
                &[0xC4, 0x22, 0xF9, 0x41, 0xF0][..],
            ),
        ] {
            let encoding = X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_phminposuw_memory_encoding()
                .unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(encoding.destination, expected_destination);
            assert_eq!(
                encoding.register_instruction.as_slice(),
                expected_register,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn reserved_register_and_nonexact_encodings_fail_closed() {
        let valid = instruction(9, 11, true);
        let mut invalid = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !0x1F) | 3;
        invalid.push(wrong_map);

        let mut wrong_prefix = valid.clone();
        wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
        invalid.push(wrong_prefix);

        let mut nonreserved_vvvv = valid.clone();
        nonreserved_vvvv[2] &= !0x08;
        invalid.push(nonreserved_vvvv);

        let mut l1 = valid.clone();
        l1[2] |= 0x04;
        invalid.push(l1);

        let mut wrong_opcode = valid.clone();
        wrong_opcode[3] = 0x40;
        invalid.push(wrong_opcode);

        let mut register = valid.clone();
        register[4] |= 0xC0;
        register.pop();
        invalid.push(register);

        let mut trailing = valid.clone();
        trailing.push(0);
        invalid.push(trailing);

        let mut truncated = valid.clone();
        truncated.pop();
        invalid.push(truncated);

        let mut forbidden_prefix = valid.clone();
        forbidden_prefix.insert(0, 0x66);
        invalid.push(forbidden_prefix);

        let mut evex = valid;
        evex[0] = 0x62;
        invalid.push(evex);

        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .vex_phminposuw_memory_encoding(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
