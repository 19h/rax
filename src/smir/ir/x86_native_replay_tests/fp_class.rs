//! Exact source-byte replay classification for EVEX VFPCLASS*.

use super::*;

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
        for spans in [
            x86_evex_fp_class_replay_spans(&block, &provenance),
            x86_evex_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert_eq!(span.needs_avx512vl, expected.0, "{bytes:02X?}");
            assert_eq!(span.needs_avx512dq, expected.1, "{bytes:02X?}");
            assert_eq!(span.needs_avx512fp16, expected.2, "{bytes:02X?}");
        }
    }
}
