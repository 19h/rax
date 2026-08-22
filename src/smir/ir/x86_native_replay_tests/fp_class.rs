//! Exact source-byte replay classification for EVEX VFPCLASS*.

use super::*;
use crate::smir::ir::types::{MemWidth, VecElementType};

type FpClassShape = (u8, u8, bool, u8);

fn shapes() -> Vec<FpClassShape> {
    let mut shapes = Vec::new();
    for (pp, w) in [(0, false), (1, false), (1, true)] {
        for ll in 0u8..=2 {
            shapes.push((0x66, pp, w, ll));
        }
        for ll in 0u8..=3 {
            shapes.push((0x67, pp, w, ll));
        }
    }
    shapes
}

fn requirements(shape: FpClassShape) -> (bool, bool, bool) {
    let (opcode, pp, _, ll) = shape;
    (opcode == 0x66 && ll != 2, pp == 1, pp == 0)
}

fn canonical_replay(instruction: X86InstructionBytes) -> X86InstructionBytes {
    instruction
        .evex_scalar_fp_class_llig_canonical_ll0()
        .unwrap_or(instruction)
}

fn encoding(shape: FpClassShape, destination: u8, source: u8, mask: u8, immediate: u8) -> [u8; 7] {
    let (opcode, pp, w, ll) = shape;
    assert!(matches!(opcode, 0x66 | 0x67));
    assert!(destination < 8 && source < 32 && mask < 8);
    assert!(opcode == 0x67 || ll < 3);
    let mut p0 = 0xF3;
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7C | pp | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08 | mask,
        opcode,
        0xC0 | (destination << 3) | (source & 0x07),
        immediate,
    ]
}

#[test]
fn classifier_covers_86016_legal_register_encodings() {
    let mut classified = 0usize;
    for shape in shapes() {
        for destination in 0u8..8 {
            for source in 0u8..32 {
                for mask in 0u8..8 {
                    for immediate in [0u8, 0xFF] {
                        let bytes = encoding(shape, destination, source, mask, immediate);
                        assert_eq!(
                            X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_register_fp_class_requirements(),
                            Some(requirements(shape)),
                            "{bytes:02X?}"
                        );
                        classified += 1;
                    }
                }
            }
        }
    }
    assert_eq!(classified, 86_016);
}

#[test]
fn classifier_rejects_reserved_or_unsafe_frontiers() {
    let valid = encoding((0x66, 1, false, 0), 2, 17, 1, 0xFF);
    let invalid: &[&[u8]] = &[
        &[0x61, 0xF3, 0x7D, 0x09, 0x66, 0xD1, 0xFF], // not EVEX
        &[0x62, 0xF2, 0x7D, 0x09, 0x66, 0xD1, 0xFF], // wrong map
        &[0x62, 0xFB, 0x7D, 0x09, 0x66, 0xD1, 0xFF], // reserved P0 bit 3
        &[0x62, 0xF3, 0x79, 0x09, 0x66, 0xD1, 0xFF], // missing fixed-one bit
        &[0x62, 0xF3, 0x7E, 0x09, 0x66, 0xD1, 0xFF], // wrong pp
        &[0x62, 0xF3, 0xFC, 0x09, 0x66, 0xD1, 0xFF], // FP16 with W1
        &[0x62, 0xF3, 0x7D, 0x09, 0x65, 0xD1, 0xFF], // wrong opcode
        &[0x62, 0xF3, 0x7D, 0x09, 0x66, 0x11, 0xFF], // memory source
        &[0x62, 0xF3, 0x7D, 0x89, 0x66, 0xD1, 0xFF], // reserved EVEX.z
        &[0x62, 0xF3, 0x7D, 0x19, 0x66, 0xD1, 0xFF], // register EVEX.b
        &[0x62, 0x73, 0x7D, 0x09, 0x66, 0xD1, 0xFF], // extended K destination R
        &[0x62, 0xE3, 0x7D, 0x09, 0x66, 0xD1, 0xFF], // extended K destination R'
        &[0x62, 0xF3, 0x7D, 0x69, 0x66, 0xD1, 0xFF], // packed L'L=3
        &[0x62, 0xF3, 0x7D, 0x09, 0x66, 0xD1],       // missing imm8
        &[0x62, 0xF3, 0x7D, 0x09, 0x66, 0xD1, 0, 0], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_fp_class_requirements(),
            None,
            "{bytes:02X?}"
        );
    }

    for encoded_vvvv in 0u8..=0x0E {
        let mut reserved_vvvv = valid;
        reserved_vvvv[2] = (reserved_vvvv[2] & !0x78) | (encoded_vvvv << 3);
        assert_eq!(
            X86InstructionBytes::new(&reserved_vvvv)
                .unwrap()
                .evex_register_fp_class_requirements(),
            None,
            "{reserved_vvvv:02X?}"
        );
    }
    let mut reserved_v_prime = valid;
    reserved_v_prime[3] &= !0x08;
    assert_eq!(
        X86InstructionBytes::new(&reserved_v_prime)
            .unwrap()
            .evex_register_fp_class_requirements(),
        None
    );

    for ll in 0u8..=3 {
        let scalar = encoding((0x67, 1, true, ll), 7, 31, 7, 0x80);
        assert_eq!(
            X86InstructionBytes::new(&scalar)
                .unwrap()
                .evex_register_fp_class_requirements(),
            Some((false, true, false)),
            "{scalar:02X?}"
        );
    }
}

#[test]
fn replay_spans_encode_vl_dq_and_fp16_requirements() {
    let pc = 0x4000;
    let mut block = SmirBlock::new(BlockId(23), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for bytes in [
        encoding((0x66, 0, false, 0), 1, 2, 0, 0x01),
        encoding((0x66, 1, false, 1), 2, 10, 1, 0x20),
        encoding((0x66, 1, true, 2), 3, 18, 2, 0x40),
        encoding((0x67, 0, false, 3), 4, 26, 7, 0x80),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let provenance = HashMap::from([((BlockId(23), pc), instruction)]);
        let expected = instruction.evex_register_fp_class_requirements().unwrap();
        let expected_instruction = canonical_replay(instruction);
        for spans in [
            x86_evex_fp_class_replay_spans(&block, &provenance),
            x86_evex_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, expected_instruction, "{bytes:02X?}");
            assert_eq!(span.needs_avx512vl, expected.0, "{bytes:02X?}");
            assert_eq!(span.needs_avx512dq, expected.1, "{bytes:02X?}");
            assert_eq!(span.needs_avx512fp16, expected.2, "{bytes:02X?}");
        }
    }
}

fn memory_encoding(
    shape: FpClassShape,
    destination: u8,
    mask: u8,
    broadcast: bool,
    immediate: u8,
    apx_base: bool,
    apx_index: bool,
) -> Vec<u8> {
    let (opcode, pp, w, ll) = shape;
    assert!(destination < 8 && mask < 8);
    assert!(!broadcast || opcode == 0x66);
    let p0 = 0xF3 | (u8::from(apx_base) << 3);
    let mut p1 = 0x7C | pp | if w { 0x80 } else { 0 };
    if apx_index {
        p1 &= !0x04;
    }
    vec![
        0x64,
        0x67,
        0x62,
        p0,
        p1,
        (ll << 5) | 0x08 | (u8::from(broadcast) << 4) | mask,
        opcode,
        (destination << 3) | 0x04,
        0x08,
        immediate,
    ]
}

fn memory_shapes() -> Vec<FpClassShape> {
    shapes()
}

#[test]
fn memory_classifier_exhausts_1_966_080_semantic_apx_and_immediate_cells() {
    let mut classified = 0usize;
    for shape in memory_shapes() {
        let (opcode, pp, _, ll) = shape;
        let elem = match pp {
            0 => VecElementType::F16,
            1 if shape.2 => VecElementType::F64,
            1 => VecElementType::F32,
            _ => unreachable!(),
        };
        let scalar = opcode == 0x67;
        let width = match (scalar, ll) {
            (true, _) | (false, 0) => VecWidth::V128,
            (false, 1) => VecWidth::V256,
            (false, 2) => VecWidth::V512,
            _ => unreachable!(),
        };
        let expected_memory_width = match elem {
            VecElementType::F16 => MemWidth::B2,
            VecElementType::F32 => MemWidth::B4,
            VecElementType::F64 => MemWidth::B8,
            _ => unreachable!(),
        };
        for destination in 0u8..8 {
            for mask in 0u8..8 {
                for broadcast in [false, true] {
                    if scalar && broadcast {
                        continue;
                    }
                    for immediate in u8::MIN..=u8::MAX {
                        for apx_base in [false, true] {
                            for apx_index in [false, true] {
                                let bytes = memory_encoding(
                                    shape,
                                    destination,
                                    mask,
                                    broadcast,
                                    immediate,
                                    apx_base,
                                    apx_index,
                                );
                                let encoded = X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .evex_fp_class_memory_encoding()
                                    .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                assert_eq!(encoded.width, width, "{bytes:02X?}");
                                assert_eq!(encoded.elem, elem, "{bytes:02X?}");
                                assert_eq!(encoded.destination, destination, "{bytes:02X?}");
                                assert_eq!(
                                    encoded.writemask,
                                    (mask != 0).then_some(mask),
                                    "{bytes:02X?}"
                                );
                                assert_eq!(encoded.immediate, immediate, "{bytes:02X?}");
                                assert_eq!(encoded.scalar, scalar, "{bytes:02X?}");
                                assert_eq!(
                                    encoded.memory_width, expected_memory_width,
                                    "{bytes:02X?}"
                                );
                                assert_eq!(
                                    encoded.needs_avx512vl,
                                    !scalar && width != VecWidth::V512,
                                    "{bytes:02X?}"
                                );
                                assert_eq!(
                                    encoded.needs_avx512dq,
                                    elem != VecElementType::F16,
                                    "{bytes:02X?}"
                                );
                                assert_eq!(
                                    encoded.needs_avx512fp16,
                                    elem == VecElementType::F16,
                                    "{bytes:02X?}"
                                );

                                let p1 = 0x7C | pp | if shape.2 { 0x80 } else { 0 };
                                let p2 = (ll << 5) | 0x08 | (u8::from(broadcast) << 4) | mask;
                                let replay_p2 = if scalar { p2 & !0x60 } else { p2 };
                                let stack = [
                                    0x62,
                                    0xF3,
                                    p1,
                                    replay_p2,
                                    opcode,
                                    (destination << 3) | 0x04,
                                    0x24,
                                    immediate,
                                ];
                                match encoded.replay {
                                    X86EvexFpClassMemoryReplay::Scalar { stack_instruction } => {
                                        assert!(scalar, "{bytes:02X?}");
                                        assert_eq!(stack_instruction.as_slice(), stack);
                                    }
                                    X86EvexFpClassMemoryReplay::Broadcast { stack_instruction } => {
                                        assert!(!scalar && broadcast, "{bytes:02X?}");
                                        assert_eq!(stack_instruction.as_slice(), stack);
                                    }
                                    X86EvexFpClassMemoryReplay::MaskedVector {
                                        stack_instruction,
                                    } => {
                                        assert!(!scalar && !broadcast && mask != 0, "{bytes:02X?}");
                                        assert_eq!(stack_instruction.as_slice(), stack);
                                    }
                                    X86EvexFpClassMemoryReplay::Vector {
                                        scratch,
                                        register_instruction,
                                    } => {
                                        assert!(!scalar && !broadcast && mask == 0, "{bytes:02X?}");
                                        assert_eq!(scratch, 0);
                                        assert_eq!(
                                            register_instruction.as_slice(),
                                            [
                                                0x62,
                                                0xF3,
                                                p1,
                                                p2,
                                                opcode,
                                                0xC0 | (destination << 3),
                                                immediate,
                                            ],
                                            "{bytes:02X?}"
                                        );
                                    }
                                }
                                classified += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 1_966_080);
}

#[test]
fn memory_classifier_rejects_reserved_nonmemory_and_trailing_frontiers() {
    let valid = memory_encoding((0x66, 1, true, 2), 7, 3, true, 0xA5, false, false);
    let evex = 2usize;
    let mut invalids = Vec::<(&str, Vec<u8>)>::new();
    let mut bytes = valid.clone();
    bytes[evex] = 0x61;
    invalids.push(("not EVEX", bytes));
    let mut bytes = valid.clone();
    bytes[evex + 1] = (bytes[evex + 1] & !7) | 2;
    invalids.push(("wrong map", bytes));
    let mut bytes = valid.clone();
    bytes[evex + 1] &= !0x80;
    invalids.push(("extended K destination R", bytes));
    let mut bytes = valid.clone();
    bytes[evex + 1] &= !0x10;
    invalids.push(("extended K destination R prime", bytes));
    let mut bytes = valid.clone();
    bytes[evex + 2] &= !0x08;
    invalids.push(("reserved vvvv", bytes));
    let mut bytes = valid.clone();
    bytes[evex + 3] &= !0x08;
    invalids.push(("reserved V prime", bytes));
    let mut bytes = valid.clone();
    bytes[evex + 3] |= 0x80;
    invalids.push(("reserved zeroing", bytes));
    let mut bytes = valid.clone();
    bytes[evex + 3] = (bytes[evex + 3] & !0x60) | 0x60;
    invalids.push(("packed L'L=3", bytes));
    let mut bytes = valid.clone();
    bytes[evex + 5] |= 0xC0;
    invalids.push(("register source", bytes));
    let mut bytes = valid.clone();
    bytes.pop();
    invalids.push(("missing immediate", bytes));
    let mut bytes = valid.clone();
    bytes.push(0);
    invalids.push(("trailing byte", bytes));

    let scalar = memory_encoding((0x67, 0, false, 3), 2, 1, false, 0xFF, true, true);
    let mut bytes = scalar;
    bytes[evex + 3] |= 0x10;
    invalids.push(("scalar EVEX.b", bytes));

    for (name, bytes) in invalids {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_fp_class_memory_encoding()
                .is_none(),
            "{name}: {bytes:02X?}"
        );
    }
}
