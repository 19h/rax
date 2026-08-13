//! Exact classifiers for VEX/EVEX packed sign/zero-extension replay.

use super::*;
use crate::smir::ir::types::VecElementType;

fn legacy_encoding(opcode: u8, rex: Option<u8>, modrm: u8) -> Vec<u8> {
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![0x66];
    bytes.extend(rex);
    bytes.extend([0x0F, 0x38, opcode, modrm]);
    bytes
}

fn legacy_shape(opcode: u8) -> (VecElementType, VecElementType, bool) {
    let (source, destination) = match opcode & 0x0F {
        0x00 => (VecElementType::I8, VecElementType::I16),
        0x01 => (VecElementType::I8, VecElementType::I32),
        0x02 => (VecElementType::I8, VecElementType::I64),
        0x03 => (VecElementType::I16, VecElementType::I32),
        0x04 => (VecElementType::I16, VecElementType::I64),
        0x05 => (VecElementType::I32, VecElementType::I64),
        _ => unreachable!(),
    };
    (source, destination, opcode < 0x30)
}

#[test]
fn legacy_classifier_covers_all_13_056_canonical_rex_register_encodings() {
    let mut classified = 0usize;
    for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
        for opcode in (0x20..=0x25).chain(0x30..=0x35) {
            let (source_element, destination_element, signed) = legacy_shape(opcode);
            for modrm in 0xC0..=0xFF {
                let bytes = legacy_encoding(opcode, rex, modrm);
                let extension = rex.unwrap_or(0);
                assert_eq!(
                    X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .legacy_register_packed_extend_replay(),
                    Some(X86LegacyPackedExtendReplay {
                        destination: ((modrm >> 3) & 7) | ((extension & 0x04) << 1),
                        source: (modrm & 7) | ((extension & 0x01) << 3),
                        source_element,
                        destination_element,
                        signed,
                    }),
                    "{bytes:02X?}"
                );
                classified += 1;
            }
        }
    }
    assert_eq!(classified, 12 * 17 * 64);
}

#[test]
fn legacy_classifier_exhausts_opcode_modrm_and_canonical_prefix_frontiers() {
    for opcode in u8::MIN..=u8::MAX {
        for modrm in u8::MIN..=u8::MAX {
            let bytes = legacy_encoding(opcode, Some(0x4F), modrm);
            let actual = X86InstructionBytes::new(&bytes)
                .unwrap()
                .legacy_register_packed_extend_replay();
            assert_eq!(
                actual.is_some(),
                matches!(opcode, 0x20..=0x25 | 0x30..=0x35) && modrm >> 6 == 3,
                "{bytes:02X?}"
            );
        }
    }

    // LLVM 23.0.0 independently decodes these W/X-ignored and R/B-extended
    // samples to the same legacy SSE4.1 mnemonics and operands.
    for (bytes, destination, source) in [
        (&[0x66, 0x0F, 0x38, 0x20, 0xCA][..], 1, 2),
        (&[0x66, 0x48, 0x0F, 0x38, 0x20, 0xCA], 1, 2),
        (&[0x66, 0x45, 0x0F, 0x38, 0x20, 0xCA], 9, 10),
        (&[0x66, 0x4F, 0x0F, 0x38, 0x35, 0xCA], 9, 10),
    ] {
        let replay = X86InstructionBytes::new(bytes)
            .unwrap()
            .legacy_register_packed_extend_replay()
            .unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!((replay.destination, replay.source), (destination, source));
    }

    let invalid: &[&[u8]] = &[
        &[0x0F, 0x38, 0x20, 0xCA],                   // missing mandatory 66
        &[0x48, 0x66, 0x0F, 0x38, 0x20, 0xCA],       // REX not final
        &[0x66, 0x66, 0x0F, 0x38, 0x20, 0xCA],       // duplicate mandatory prefix
        &[0x67, 0x66, 0x0F, 0x38, 0x20, 0xCA],       // reserved register-form 67
        &[0x64, 0x66, 0x0F, 0x38, 0x20, 0xCA],       // segment override
        &[0xF3, 0x66, 0x0F, 0x38, 0x20, 0xCA],       // reserved repeat prefix
        &[0x66, 0xD5, 0x00, 0x0F, 0x38, 0x20, 0xCA], // REX2
        &[0x66, 0x0F, 0x38, 0x20, 0x0A],             // memory source
        &[0x66, 0x0F, 0x38, 0x1F, 0xCA],             // neighboring opcode
        &[0x66, 0x0F, 0x38, 0x26, 0xCA],             // neighboring opcode
        &[0x66, 0x0F, 0x38, 0x2F, 0xCA],             // gap between families
        &[0x66, 0x0F, 0x38, 0x36, 0xCA],             // neighboring opcode
        &[0x66, 0x0F, 0x38, 0x20],                   // missing ModR/M
        &[0x66, 0x0F, 0x38, 0x20, 0xCA, 0x00],       // trailing byte
        &[0xC4, 0xE2, 0x79, 0x20, 0xCA],             // VEX
        &[0x62, 0xF2, 0x7D, 0x08, 0x20, 0xCA],       // EVEX
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .legacy_register_packed_extend_replay(),
            None,
            "{bytes:02X?}"
        );
    }
}

fn legacy_function(
    bytes: &[u8],
    level: crate::smir::optimize::OptLevel,
) -> crate::smir::ir::SmirFunction {
    use crate::smir::ir::types::{FunctionId, SourceArch};
    use crate::smir::ir::{SmirFunction, Terminator};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    const PC: u64 = 0xC4E0;
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("legacy packed-extension provenance"),
    );
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn assert_legacy_span(function: &crate::smir::ir::SmirFunction, bytes: &[u8]) {
    for spans in [
        x86_legacy_packed_extend_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes),
    ] {
        let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!(span.end, function.blocks[0].ops.len(), "{bytes:02X?}");
        assert_eq!(span.instruction.as_slice(), bytes, "{bytes:02X?}");
        assert!(!span.needs_avx512vl, "{bytes:02X?}");
        assert!(!span.needs_avx512dq, "{bytes:02X?}");
        assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        assert!(!span.preserve_mxcsr_de, "{bytes:02X?}");
    }
}

fn assert_legacy_rejected(function: &crate::smir::ir::SmirFunction, label: &str) {
    assert!(
        x86_legacy_packed_extend_replay_spans(
            &function.blocks[0],
            &function.x86_instruction_bytes,
        )
        .is_empty(),
        "dedicated span admitted {label}"
    );
    assert!(
        x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes).is_empty(),
        "aggregate span admitted {label}"
    );
}

#[test]
fn legacy_exact_graph_validator_covers_all_shapes_and_rejects_every_op_mutation() {
    use crate::smir::ir::Terminator;
    use crate::smir::ir::ops::X86OpHint;
    use crate::smir::optimize::OptLevel;

    for opcode in (0x20..=0x25).chain(0x30..=0x35) {
        let (_, destination_element, _) = legacy_shape(opcode);
        let lanes = VecWidth::V128.lanes(destination_element) as usize;
        let bytes = legacy_encoding(opcode, Some(0x45), 0xCA);
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let function = legacy_function(&bytes, level);
            assert_eq!(function.blocks[0].ops.len(), 4 * lanes + 2);
            assert_legacy_span(&function, &bytes);
        }

        let baseline = legacy_function(&bytes, OptLevel::O0);
        for index in 0..baseline.blocks[0].ops.len() {
            let mut mutated = baseline.clone();
            mutated.blocks[0].ops[index].kind = OpKind::Nop;
            assert_legacy_rejected(&mutated, &format!("opcode {opcode:02X} op {index}"));

            let mut hinted = baseline.clone();
            hinted.blocks[0].ops[index].x86_hint = Some(X86OpHint::RexByteReg);
            assert_legacy_rejected(&hinted, &format!("opcode {opcode:02X} hinted op {index}"));
        }

        let temporary = baseline.blocks[0]
            .ops
            .iter()
            .flat_map(|op| op.kind.dests())
            .find(|register| matches!(register, VReg::Virtual(_)))
            .expect("legacy packed-extension temporary");
        let mut escaped = baseline.clone();
        escaped.blocks[0].set_terminator(Terminator::Return {
            values: vec![temporary],
        });
        assert_legacy_rejected(&escaped, &format!("opcode {opcode:02X} escaped temporary"));
    }

    let source = legacy_encoding(0x20, Some(0x45), 0xCA);
    let baseline = legacy_function(&source, OptLevel::O0);
    for (label, metadata) in [
        (
            "wrong element shape",
            legacy_encoding(0x21, Some(0x45), 0xCA),
        ),
        ("wrong destination", legacy_encoding(0x20, Some(0x41), 0xCA)),
        ("wrong source", legacy_encoding(0x20, Some(0x44), 0xCA)),
        ("memory provenance", legacy_encoding(0x20, Some(0x45), 0x0A)),
    ] {
        let mut malformed = baseline.clone();
        malformed.x86_instruction_bytes.insert(
            (BlockId(0), 0xC4E0),
            X86InstructionBytes::new(&metadata).unwrap(),
        );
        assert_legacy_rejected(&malformed, label);
    }
}

fn vex_encoding(p0: u8, p1: u8, opcode: u8, modrm: u8) -> [u8; 5] {
    [0xC4, p0, p1, opcode, modrm]
}

#[test]
fn vex_classifier_covers_all_24576_legal_register_encodings_and_destinations() {
    let mut classified = 0usize;
    for p0 in u8::MIN..=u8::MAX {
        if p0 & 0x1F != 2 {
            continue;
        }
        for p1 in u8::MIN..=u8::MAX {
            if p1 & 0x78 != 0x78 || p1 & 0x03 != 1 {
                continue;
            }
            for opcode in (0x20..=0x25).chain(0x30..=0x35) {
                for modrm in 0xC0..=0xFF {
                    let bytes = vex_encoding(p0, p1, opcode, modrm);
                    let instruction = X86InstructionBytes::new(&bytes).unwrap();
                    assert_eq!(
                        instruction.vex_register_packed_extend_needs_avx2(),
                        Some(p1 & 0x04 != 0),
                        "{bytes:02X?}"
                    );
                    assert_eq!(
                        instruction.vex_packed_extend_destination_index(),
                        Some(((modrm >> 3) & 7) + if p0 & 0x80 == 0 { 8 } else { 0 }),
                        "{bytes:02X?}"
                    );
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 24_576);
}

#[test]
fn vex_classifier_exhausts_prefix_opcode_and_modrm_frontiers() {
    for p0 in u8::MIN..=u8::MAX {
        let bytes = vex_encoding(p0, 0x79, 0x20, 0xCA);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_packed_extend_needs_avx2(),
            (p0 & 0x1F == 2).then_some(false),
            "{bytes:02X?}"
        );
    }

    for p1 in u8::MIN..=u8::MAX {
        let bytes = vex_encoding(0xE2, p1, 0x20, 0xCA);
        let expected = (p1 & 0x78 == 0x78 && p1 & 0x03 == 1).then_some(p1 & 0x04 != 0);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_packed_extend_needs_avx2(),
            expected,
            "{bytes:02X?}"
        );
    }

    for opcode in u8::MIN..=u8::MAX {
        let bytes = vex_encoding(0xE2, 0x79, opcode, 0xCA);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_packed_extend_needs_avx2(),
            matches!(opcode, 0x20..=0x25 | 0x30..=0x35).then_some(false),
            "{bytes:02X?}"
        );
    }

    for modrm in u8::MIN..=u8::MAX {
        let bytes = vex_encoding(0xE2, 0xFD, 0x35, modrm);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_packed_extend_needs_avx2(),
            (modrm >> 6 == 3).then_some(true),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn vex_classifier_accepts_llvm_samples_wig_and_ignored_x_and_rejects_neighbors() {
    // LLVM 23.0.0 independently assembled the first four samples. It also
    // independently decoded the W=1 and X'=0 mutations to the same canonical
    // mnemonics and operands as their W=0/X'=1 counterparts.
    for (bytes, needs_avx2, destination) in [
        (&[0xC4, 0xE2, 0x79, 0x20, 0xCA][..], false, 1),
        (&[0xC4, 0x42, 0x7D, 0x20, 0xCA], true, 9),
        (&[0xC4, 0xE2, 0x79, 0x35, 0xCA], false, 1),
        (&[0xC4, 0x42, 0x7D, 0x35, 0xCA], true, 9),
        (&[0xC4, 0xE2, 0xF9, 0x20, 0xCA], false, 1),
        (&[0xC4, 0xA2, 0x79, 0x20, 0xCA], false, 1),
        (&[0xC4, 0x42, 0xFD, 0x35, 0xCA], true, 9),
        (&[0xC4, 0x02, 0x7D, 0x35, 0xCA], true, 9),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert_eq!(
            instruction.vex_register_packed_extend_needs_avx2(),
            Some(needs_avx2),
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_packed_extend_destination_index(),
            Some(destination),
            "{bytes:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0xC5, 0xF9, 0x20, 0xCA],       // two-byte VEX cannot select map 0F38
        &[0xC4, 0xE1, 0x79, 0x20, 0xCA], // map 0F
        &[0xC4, 0xE3, 0x79, 0x20, 0xCA], // map 0F3A
        &[0xC4, 0xE2, 0x78, 0x20, 0xCA], // missing mandatory 66
        &[0xC4, 0xE2, 0x69, 0x20, 0xCA], // nonreserved VEX.vvvv
        &[0xC4, 0xE2, 0x79, 0x1F, 0xCA], // unrelated opcode
        &[0xC4, 0xE2, 0x79, 0x26, 0xCA], // unrelated opcode
        &[0xC4, 0xE2, 0x79, 0x20, 0x0A], // memory source
        &[0xC4, 0xE2, 0x79, 0x20],       // missing ModR/M
        &[0xC4, 0xE2, 0x79, 0x20, 0xCA, 0x00], // trailing byte
        &[0x62, 0xF2, 0x7D, 0x08, 0x20, 0xCA], // EVEX, not VEX
    ];
    for bytes in invalid {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        assert_eq!(
            instruction.vex_register_packed_extend_needs_avx2(),
            None,
            "{bytes:02X?}"
        );
        assert_eq!(
            instruction.vex_packed_extend_destination_index(),
            None,
            "{bytes:02X?}"
        );
    }
}

fn vex_memory_shape(opcode: u8) -> (VecElementType, VecElementType, bool) {
    let signed = opcode < 0x30;
    let (source, destination) = match opcode & 0x0F {
        0x00 => (VecElementType::I8, VecElementType::I16),
        0x01 => (VecElementType::I8, VecElementType::I32),
        0x02 => (VecElementType::I8, VecElementType::I64),
        0x03 => (VecElementType::I16, VecElementType::I32),
        0x04 => (VecElementType::I16, VecElementType::I64),
        0x05 => (VecElementType::I32, VecElementType::I64),
        _ => unreachable!(),
    };
    (source, destination, signed)
}

fn vex_memory_encoding(
    destination: u8,
    base: u8,
    opcode: u8,
    width: VecWidth,
    w: bool,
    encoded_x: bool,
) -> Vec<u8> {
    assert!(destination < 16 && base < 16);
    assert!(matches!(opcode, 0x20..=0x25 | 0x30..=0x35));
    vec![
        0xC4,
        (if destination < 8 { 0x80 } else { 0 })
            | (u8::from(encoded_x) << 6)
            | (if base < 8 { 0x20 } else { 0 })
            | 2,
        (u8::from(w) << 7) | 0x78 | (u8::from(width == VecWidth::V256) << 2) | 1,
        opcode,
        0x40 | ((destination & 7) << 3) | (base & 7),
        0x20,
    ]
}

#[test]
fn vex_memory_classifier_covers_all_3072_destination_base_shape_width_and_w_cells() {
    let mut classified = 0usize;
    for destination in 0..16 {
        for base in [3, 11] {
            for opcode in (0x20..=0x25).chain(0x30..=0x35) {
                let (source_element, destination_element, signed) = vex_memory_shape(opcode);
                for width in [VecWidth::V128, VecWidth::V256] {
                    for w in [false, true] {
                        for encoded_x in [false, true] {
                            let bytes =
                                vex_memory_encoding(destination, base, opcode, width, w, encoded_x);
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_memory_packed_extend_fields(),
                                Some((
                                    destination,
                                    source_element,
                                    destination_element,
                                    width,
                                    signed,
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
    assert_eq!(classified, 16 * 2 * 12 * 2 * 2 * 2);
}

#[test]
fn vex_memory_classifier_accepts_llvm_23_address_and_opcode_encodings() {
    let samples: &[(
        &[u8],
        u8,
        VecElementType,
        VecElementType,
        VecWidth,
        bool,
        u8,
    )] = &[
        (
            &[0xC4, 0x42, 0x79, 0x20, 0x4B, 0x20],
            9,
            VecElementType::I8,
            VecElementType::I16,
            VecWidth::V128,
            true,
            0x20,
        ),
        (
            &[0xC4, 0x02, 0x7D, 0x21, 0xBC, 0xEC, 0x44, 0x33, 0x22, 0x11],
            15,
            VecElementType::I8,
            VecElementType::I32,
            VecWidth::V256,
            true,
            0x21,
        ),
        (
            &[
                0x64, 0xC4, 0x62, 0x79, 0x22, 0x34, 0x8D, 0x44, 0x33, 0x22, 0x11,
            ],
            14,
            VecElementType::I8,
            VecElementType::I64,
            VecWidth::V128,
            true,
            0x22,
        ),
        (
            &[0xC4, 0x62, 0x7D, 0x23, 0x2D, 0x44, 0x33, 0x22, 0x11],
            13,
            VecElementType::I16,
            VecElementType::I32,
            VecWidth::V256,
            true,
            0x23,
        ),
        (
            &[0x65, 0xC4, 0x42, 0x79, 0x24, 0x62, 0xE0],
            12,
            VecElementType::I16,
            VecElementType::I64,
            VecWidth::V128,
            true,
            0x24,
        ),
        (
            &[0xC4, 0x02, 0x7D, 0x25, 0x5C, 0x48, 0x20],
            11,
            VecElementType::I32,
            VecElementType::I64,
            VecWidth::V256,
            true,
            0x25,
        ),
        (
            &[0xC4, 0x42, 0x7D, 0x30, 0x53, 0x20],
            10,
            VecElementType::I8,
            VecElementType::I16,
            VecWidth::V256,
            false,
            0x30,
        ),
        (
            &[0xC4, 0x02, 0x79, 0x31, 0x8C, 0xEC, 0x44, 0x33, 0x22, 0x11],
            9,
            VecElementType::I8,
            VecElementType::I32,
            VecWidth::V128,
            false,
            0x31,
        ),
        (
            &[
                0x64, 0xC4, 0x62, 0x7D, 0x32, 0x04, 0x8D, 0x44, 0x33, 0x22, 0x11,
            ],
            8,
            VecElementType::I8,
            VecElementType::I64,
            VecWidth::V256,
            false,
            0x32,
        ),
        (
            &[0xC4, 0xE2, 0x79, 0x33, 0x3D, 0x44, 0x33, 0x22, 0x11],
            7,
            VecElementType::I16,
            VecElementType::I32,
            VecWidth::V128,
            false,
            0x33,
        ),
        (
            &[0x65, 0xC4, 0xC2, 0x7D, 0x34, 0x72, 0xE0],
            6,
            VecElementType::I16,
            VecElementType::I64,
            VecWidth::V256,
            false,
            0x34,
        ),
        (
            &[0xC4, 0x82, 0x79, 0x35, 0x6C, 0x48, 0x20],
            5,
            VecElementType::I32,
            VecElementType::I64,
            VecWidth::V128,
            false,
            0x35,
        ),
        (
            &[0x67, 0xC4, 0xE2, 0x79, 0x20, 0x4C, 0x77, 0x20],
            1,
            VecElementType::I8,
            VecElementType::I16,
            VecWidth::V128,
            true,
            0x20,
        ),
    ];
    for &(bytes, destination, source_element, destination_element, width, signed, opcode) in samples
    {
        let metadata = X86InstructionBytes::new(bytes).unwrap();
        assert_eq!(
            metadata.vex_memory_packed_extend_fields(),
            Some((
                destination,
                source_element,
                destination_element,
                width,
                signed,
                opcode,
                false,
            )),
            "{bytes:02X?}"
        );

        let mut w1 = bytes.to_vec();
        let vex = w1.iter().position(|byte| *byte == 0xC4).unwrap();
        w1[vex + 2] |= 0x80;
        assert_eq!(
            X86InstructionBytes::new(&w1)
                .unwrap()
                .vex_memory_packed_extend_fields(),
            Some((
                destination,
                source_element,
                destination_element,
                width,
                signed,
                opcode,
                true,
            )),
            "{w1:02X?}"
        );
    }
}

#[test]
fn vex_memory_classifier_rejects_every_semantic_and_length_frontier() {
    let valid = vex_memory_encoding(9, 11, 0x20, VecWidth::V256, true, true);
    let mut invalid = Vec::new();

    let mut wrong_map = valid.clone();
    wrong_map[1] = (wrong_map[1] & !0x1F) | 1;
    invalid.push(wrong_map);

    let mut wrong_prefix = valid.clone();
    wrong_prefix[2] = (wrong_prefix[2] & !3) | 2;
    invalid.push(wrong_prefix);

    let mut nonreserved_vvvv = valid.clone();
    nonreserved_vvvv[2] &= !0x08;
    invalid.push(nonreserved_vvvv);

    let mut wrong_opcode = valid.clone();
    wrong_opcode[3] = 0x26;
    invalid.push(wrong_opcode);

    let mut register_source = valid.clone();
    register_source[4] |= 0xC0;
    register_source.truncate(5);
    invalid.push(register_source);

    let mut missing_displacement = valid.clone();
    missing_displacement.pop();
    invalid.push(missing_displacement);

    let mut trailing = valid.clone();
    trailing.push(0);
    invalid.push(trailing);

    let mut forbidden_legacy_prefix = valid;
    forbidden_legacy_prefix.insert(0, 0xF3);
    invalid.push(forbidden_legacy_prefix);

    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_memory_packed_extend_fields(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn vex_packed_extend_replay_spans_require_no_avx512_features() {
    let pc = 0x30F0;
    let mut block = SmirBlock::new(BlockId(12), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for bytes in [
        &[0xC4, 0xE2, 0x79, 0x20, 0xCA][..],
        &[0xC4, 0x42, 0xFD, 0x35, 0xCA],
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(12), pc), instruction)]);
        for spans in [
            x86_vex_packed_extend_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert!(!span.needs_avx512vl, "{bytes:02X?}");
            assert!(!span.needs_avx512dq, "{bytes:02X?}");
            assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        }
    }
}

type PackedExtendShape = (u8, bool, u8);

fn packed_extend_shapes() -> Vec<PackedExtendShape> {
    let mut shapes = Vec::new();
    for opcode in (0x20..=0x25).chain(0x30..=0x35) {
        let widths: &[bool] = if matches!(opcode, 0x25 | 0x35) {
            &[false]
        } else {
            &[false, true]
        };
        for &w in widths {
            for ll in 0..=2 {
                shapes.push((opcode, w, ll));
            }
        }
    }
    shapes
}

fn generated_packed_extend_encoding(shape: PackedExtendShape, rm: u8) -> [u8; 6] {
    let (opcode, w, ll) = shape;
    let mut p0 = 0xF2;
    if rm & 0x08 != 0 {
        p0 &= !0x20;
    }
    if rm & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7D | if w { 0x80 } else { 0 },
        (ll << 5) | 0x09,
        opcode,
        0xC8 | (rm & 0x07),
    ]
}

#[test]
fn evex_classifier_covers_264_register_encodings() {
    let shapes = packed_extend_shapes();
    assert_eq!(shapes.len(), 66);

    let mut register_encodings = 0usize;
    for shape in shapes {
        for rm in [0, 8, 16, 24] {
            let bytes = generated_packed_extend_encoding(shape, rm);
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_packed_extend_needs_vl(),
                Some(shape.2 != 2),
                "{bytes:02X?}"
            );
            register_encodings += 1;
        }

        let mut memory = generated_packed_extend_encoding(shape, 0);
        memory[5] = 0x08;
        assert_eq!(
            X86InstructionBytes::new(&memory)
                .unwrap()
                .evex_register_packed_extend_needs_vl(),
            None,
            "{memory:02X?}"
        );
    }
    assert_eq!(register_encodings, 264);

    // Independent LLVM encodings cover all twelve mnemonics and every EVEX
    // destination/source register-extension channel.
    for bytes in [
        &[0x62, 0x02, 0x7D, 0xC9, 0x20, 0xCA][..],
        &[0x62, 0x02, 0x7D, 0xC9, 0x21, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x22, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x23, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x24, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x25, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x30, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x31, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x32, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x33, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x34, 0xCA],
        &[0x62, 0x02, 0x7D, 0xC9, 0x35, 0xCA],
        // Intel WIG forms with W1, independently decoded by LLVM.
        &[0x62, 0xF2, 0xFD, 0x49, 0x20, 0xC8],
        &[0x62, 0xF2, 0xFD, 0x49, 0x34, 0xC8],
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_packed_extend_needs_vl(),
            Some(false),
            "{bytes:02X?}"
        );
    }

    let unmasked = [0x62, 0xF2, 0x7D, 0x48, 0x20, 0xC8];
    assert_eq!(
        X86InstructionBytes::new(&unmasked)
            .unwrap()
            .evex_register_packed_extend_needs_vl(),
        Some(false)
    );
}

#[test]
fn evex_classifier_rejects_every_reserved_frontier() {
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF2, 0x7D, 0x09, 0x20, 0xC8],       // not EVEX
        &[0x62, 0xF1, 0x7D, 0x09, 0x20, 0xC8],       // map 1
        &[0x62, 0xF2, 0x79, 0x09, 0x20, 0xC8],       // missing fixed-one bit
        &[0x62, 0xF2, 0x7C, 0x09, 0x20, 0xC8],       // missing mandatory 66
        &[0x62, 0xF2, 0x6D, 0x09, 0x20, 0xC8],       // nonreserved vvvv
        &[0x62, 0xF2, 0x7D, 0x01, 0x20, 0xC8],       // nonreserved V'
        &[0x62, 0xF2, 0xFD, 0x09, 0x25, 0xC8],       // VPMOVSXDQ with W1
        &[0x62, 0xF2, 0xFD, 0x09, 0x35, 0xC8],       // VPMOVZXDQ with W1
        &[0x62, 0xF2, 0x7D, 0x19, 0x20, 0xC8],       // EVEX.b
        &[0x62, 0xF2, 0x7D, 0x69, 0x20, 0xC8],       // reserved L'L=3
        &[0x62, 0xF2, 0x7D, 0x88, 0x20, 0xC8],       // {z} with k0
        &[0x62, 0xF2, 0x7D, 0x09, 0x20, 0x08],       // memory operand
        &[0x62, 0xF2, 0x7D, 0x09, 0x26, 0xC8],       // unrelated opcode
        &[0x62, 0xF2, 0x7D, 0x09, 0x20],             // missing ModR/M
        &[0x62, 0xF2, 0x7D, 0x09, 0x20, 0xC8, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_packed_extend_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn evex_replay_spans_require_only_vl_for_sub_512_widths() {
    let pc = 0x3100;
    let mut block = SmirBlock::new(BlockId(12), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for (bytes, needs_vl) in [
        (&[0x62, 0xF2, 0x7D, 0x09, 0x20, 0xC8][..], true),
        (&[0x62, 0xF2, 0xFD, 0x29, 0x34, 0xC8], true),
        (&[0x62, 0xF2, 0x7D, 0x49, 0x35, 0xC8], false),
    ] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(12), pc), instruction)]);
        for spans in [
            x86_evex_packed_extend_replay_spans(&block, &provenance),
            x86_evex_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert_eq!(span.needs_avx512vl, needs_vl, "{bytes:02X?}");
            assert!(!span.needs_avx512dq, "{bytes:02X?}");
            assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        }
    }
}
