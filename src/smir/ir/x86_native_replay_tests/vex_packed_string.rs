//! Exact classifier and span tests for AVX VEX packed-string replay.

use super::*;
use crate::smir::ir::ops::X86PackedStringKind;
use crate::smir::ir::types::OpWidth;

fn encoding(opcode: u8, w: bool, r: bool, x: bool, b: bool, modrm: u8, imm: u8) -> [u8; 6] {
    assert!(matches!(opcode, 0x60..=0x63));
    [
        0xC4,
        (if r { 0 } else { 0x80 }) | (if x { 0 } else { 0x40 }) | (if b { 0 } else { 0x20 }) | 3,
        (if w { 0x80 } else { 0 }) | 0x79,
        opcode,
        modrm,
        imm,
    ]
}

fn memory_encoding(opcode: u8, w: bool, source1: u8, base: u8, imm: u8) -> Vec<u8> {
    assert!(matches!(opcode, 0x60..=0x63));
    assert!(source1 < 16 && base < 16);
    let mut bytes = vec![
        0xC4,
        (if source1 < 8 { 0x80 } else { 0 }) | 0x40 | (if base < 8 { 0x20 } else { 0 }) | 3,
        (if w { 0x80 } else { 0 }) | 0x79,
        opcode,
        0x40 | ((source1 & 7) << 3) | (base & 7),
    ];
    if base & 7 == 4 {
        bytes.push(0x24);
    }
    bytes.extend([0x20, imm]);
    bytes
}

#[test]
fn classifier_accepts_all_1_048_576_canonical_register_encodings() {
    let mut classified = 0usize;
    for opcode in 0x60..=0x63 {
        for w in [false, true] {
            for r in [false, true] {
                for x in [false, true] {
                    for b in [false, true] {
                        for reg_rm in 0u8..=0x3F {
                            for imm in u8::MIN..=u8::MAX {
                                let bytes = encoding(opcode, w, r, x, b, 0xC0 | reg_rm, imm);
                                assert!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .is_vex_register_packed_string_compare(),
                                    "{bytes:02X?}"
                                );
                                assert_eq!(
                                    X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .vex_register_packed_string_returns_mask(),
                                    Some(matches!(opcode, 0x60 | 0x62)),
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
    assert_eq!(classified, 1_048_576);

    // Independently assembled by LLVM 21.1.8.
    for bytes in [
        [0xC4, 0xE3, 0x79, 0x60, 0xCA, 0x00],
        [0xC4, 0xE3, 0x79, 0x61, 0xCA, 0x00],
        [0xC4, 0xE3, 0x79, 0x62, 0xCA, 0x00],
        [0xC4, 0xE3, 0x79, 0x63, 0xCA, 0x00],
    ] {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_packed_string_compare(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn memory_classifier_accepts_all_524_288_family_operand_and_immediate_cells() {
    let mut classified = 0usize;
    for opcode in 0x60..=0x63 {
        for w in [false, true] {
            for source1 in 0..16 {
                for base in 0..16 {
                    for immediate in u8::MIN..=u8::MAX {
                        let bytes = memory_encoding(opcode, w, source1, base, immediate);
                        let encoding = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .vex_packed_string_memory_encoding()
                            .unwrap_or_else(|| panic!("{bytes:02X?}"));
                        let expected_kind = match opcode {
                            0x60 => X86PackedStringKind::ExplicitMask,
                            0x61 => X86PackedStringKind::ExplicitIndex,
                            0x62 => X86PackedStringKind::ImplicitMask,
                            0x63 => X86PackedStringKind::ImplicitIndex,
                            _ => unreachable!(),
                        };
                        let scratch = (1..16u8).find(|candidate| *candidate != source1).unwrap();
                        assert_eq!(encoding.kind, expected_kind, "{bytes:02X?}");
                        assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                        assert_eq!(encoding.scratch, scratch, "{bytes:02X?}");
                        assert_eq!(encoding.immediate, immediate, "{bytes:02X?}");
                        assert_eq!(
                            encoding.length_width,
                            if expected_kind.is_explicit() && w {
                                OpWidth::W64
                            } else {
                                OpWidth::W32
                            },
                            "{bytes:02X?}"
                        );
                        assert_eq!(encoding.memory_size, 16, "{bytes:02X?}");
                        assert!(
                            encoding
                                .register_instruction
                                .is_vex_register_packed_string_compare(),
                            "{bytes:02X?}"
                        );
                        assert_eq!(
                            encoding
                                .register_instruction
                                .vex_register_packed_string_returns_mask(),
                            Some(expected_kind.returns_mask()),
                            "{bytes:02X?}"
                        );
                        classified += 1;
                    }
                }
            }
        }
    }
    assert_eq!(classified, 524_288);
}

#[test]
fn memory_classifier_accepts_complete_segment_address_and_displacement_shapes() {
    for (name, bytes, kind, source1, length_width) in [
        (
            "FS addr32 extended SIB explicit mask",
            &[
                0x64, 0x67, 0xC4, 0x03, 0x79, 0x60, 0x8C, 0x7E, 0x11, 0x22, 0x33, 0x44, 0xFF,
            ][..],
            X86PackedStringKind::ExplicitMask,
            9,
            OpWidth::W32,
        ),
        (
            "SS addr32 extended SIB implicit index",
            &[
                0x36, 0x67, 0xC4, 0x03, 0xF9, 0x63, 0x8C, 0x7E, 0x11, 0x22, 0x33, 0x44, 0x80,
            ][..],
            X86PackedStringKind::ImplicitIndex,
            9,
            OpWidth::W32,
        ),
        (
            "RIP-relative implicit mask",
            &[0xC4, 0xE3, 0x79, 0x62, 0x0D, 0x11, 0x22, 0x33, 0x44, 0x40][..],
            X86PackedStringKind::ImplicitMask,
            1,
            OpWidth::W32,
        ),
        (
            "RBP displacement explicit 64-bit index",
            &[0xC4, 0xE3, 0xF9, 0x61, 0x4D, 0x20, 0x00][..],
            X86PackedStringKind::ExplicitIndex,
            1,
            OpWidth::W64,
        ),
    ] {
        let encoding = X86InstructionBytes::new(bytes)
            .unwrap()
            .vex_packed_string_memory_encoding()
            .unwrap_or_else(|| panic!("{name}: {bytes:02X?}"));
        assert_eq!(encoding.kind, kind, "{name}");
        assert_eq!(encoding.source1, source1, "{name}");
        assert_eq!(encoding.length_width, length_width, "{name}");
        assert_eq!(encoding.memory_size, 16, "{name}");
        assert!(
            encoding
                .register_instruction
                .is_vex_register_packed_string_compare(),
            "{name}"
        );
    }
}

#[test]
fn classifier_rejects_every_structural_frontier() {
    let canonical = encoding(0x60, true, true, true, true, 0xCA, 0xA5);
    let mut invalid = vec![
        canonical[..5].to_vec(),
        canonical.iter().copied().chain([0]).collect(),
        [
            0xC5,
            canonical[1],
            canonical[2],
            canonical[3],
            canonical[4],
            canonical[5],
        ]
        .to_vec(),
    ];
    for (index, value) in [
        (1, (canonical[1] & !0x1F) | 1), // map 0F
        (1, (canonical[1] & !0x1F) | 2), // map 0F38
        (2, canonical[2] & !0x01),       // no mandatory 66H
        (2, canonical[2] | 0x04),        // VEX.L = 1
        (2, canonical[2] & !0x08),       // VEX.vvvv != 1111b
        (3, 0x5F),                       // neighboring opcode
        (3, 0x64),                       // neighboring opcode
        (4, canonical[4] & 0x3F),        // memory source
    ] {
        let mut bytes = canonical;
        bytes[index] = value;
        invalid.push(bytes.to_vec());
    }
    for bytes in invalid {
        assert!(
            !X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_packed_string_compare(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn memory_classifier_rejects_every_structural_frontier() {
    let canonical = memory_encoding(0x60, true, 9, 11, 0xA5);
    let mut invalid = vec![
        canonical[..canonical.len() - 1].to_vec(),
        canonical.iter().copied().chain([0]).collect(),
    ];
    for (index, value) in [
        (0, 0xC5),
        (1, (canonical[1] & !0x1F) | 1),
        (1, (canonical[1] & !0x1F) | 2),
        (2, canonical[2] & !0x01),
        (2, canonical[2] | 0x04),
        (2, canonical[2] & !0x08),
        (3, 0x5F),
        (3, 0x64),
        (4, canonical[4] | 0xC0),
    ] {
        let mut bytes = canonical.clone();
        bytes[index] = value;
        invalid.push(bytes);
    }
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_packed_string_memory_encoding(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn dedicated_and_aggregate_spans_require_exact_contiguous_provenance() {
    let pc = 0x6063;
    let instruction =
        X86InstructionBytes::new(&encoding(0x63, true, true, false, true, 0xFF, 0x7F)).unwrap();
    let mut block = SmirBlock::new(BlockId(4), pc);
    block.push_op(SmirOp::new(
        OpId(0),
        pc,
        OpKind::X86PackedStringCompare {
            dst: VReg::Arch(crate::smir::ir::types::ArchReg::X86(
                crate::smir::ir::types::X86Reg::Rcx,
            )),
            src1: VReg::Arch(crate::smir::ir::types::ArchReg::X86(
                crate::smir::ir::types::X86Reg::Xmm(15),
            )),
            src2: VReg::Arch(crate::smir::ir::types::ArchReg::X86(
                crate::smir::ir::types::X86Reg::Xmm(15),
            )),
            len1: None,
            len2: None,
            length_width: OpWidth::W32,
            kind: X86PackedStringKind::ImplicitIndex,
            imm: 0x7F,
            zero_upper: false,
        },
    ));
    let provenance = std::collections::HashMap::from([((block.id, pc), instruction)]);

    for spans in [
        x86_vex_packed_string_replay_spans(&block, &provenance),
        x86_native_replay_spans(&block, &provenance),
    ] {
        let span = spans.get(&0).expect("exact VEX replay span");
        assert_eq!(span.end, 1);
        assert_eq!(span.instruction, instruction);
        assert!(!span.needs_avx512vl);
        assert!(!span.needs_avx512dq);
        assert!(!span.needs_avx512fp16);
    }
    assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());

    block.push_op(SmirOp::new(OpId(1), pc + 6, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(2), pc, OpKind::Nop));
    assert!(x86_native_replay_spans(&block, &provenance).is_empty());
}

#[test]
fn span_validation_rejects_every_semantic_field_mutation_and_extra_operation() {
    use crate::smir::ir::types::{ArchReg, X86Reg};

    let pc = 0x6063;
    let instruction =
        X86InstructionBytes::new(&encoding(0x60, true, true, false, true, 0xCA, 0xA5)).unwrap();
    let canonical = OpKind::X86PackedStringCompare {
        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
        src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
        src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
        len1: Some(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
        len2: Some(VReg::Arch(ArchReg::X86(X86Reg::Rdx))),
        length_width: OpWidth::W64,
        kind: X86PackedStringKind::ExplicitMask,
        imm: 0xA5,
        zero_upper: true,
    };
    let provenance = std::collections::HashMap::from([((BlockId(0), pc), instruction)]);
    let admitted = |kind: OpKind, hint| {
        let mut block = SmirBlock::new(BlockId(0), pc);
        let mut operation = SmirOp::new(OpId(0), pc, kind);
        operation.x86_hint = hint;
        block.push_op(operation);
        !x86_native_replay_spans(&block, &provenance).is_empty()
    };
    assert!(admitted(canonical.clone(), None));

    let mut mutations = Vec::new();
    for field in 0..9 {
        let mut mutated = canonical.clone();
        let OpKind::X86PackedStringCompare {
            dst,
            src1,
            src2,
            len1,
            len2,
            length_width,
            kind,
            imm,
            zero_upper,
        } = &mut mutated
        else {
            unreachable!()
        };
        match field {
            0 => *dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            1 => *src1 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(8))),
            2 => *src2 = VReg::Arch(ArchReg::X86(X86Reg::Xmm(11))),
            3 => *len1 = None,
            4 => *len2 = None,
            5 => *length_width = OpWidth::W32,
            6 => *kind = X86PackedStringKind::ImplicitMask,
            7 => *imm ^= 1,
            8 => *zero_upper = false,
            _ => unreachable!(),
        }
        mutations.push(mutated);
    }
    for mutated in mutations {
        assert!(!admitted(mutated, None));
    }
    assert!(!admitted(
        canonical.clone(),
        Some(crate::smir::ir::ops::X86OpHint::Mulx)
    ));

    let mut extra = SmirBlock::new(BlockId(0), pc);
    extra.push_op(SmirOp::new(OpId(0), pc, canonical));
    extra.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
    assert!(x86_native_replay_spans(&extra, &provenance).is_empty());
}
