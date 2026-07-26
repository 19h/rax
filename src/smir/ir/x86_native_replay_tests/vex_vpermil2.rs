//! Exact classifier and span tests for AMD XOP VPERMIL2 replay.

use super::*;

fn encoding(
    extension_bits: u8,
    w: bool,
    encoded_vvvv: u8,
    l: bool,
    opcode: u8,
    modrm: u8,
    immediate: u8,
) -> [u8; 6] {
    assert_eq!(extension_bits & !0xE0, 0);
    assert!(encoded_vvvv < 16);
    [
        0xC4,
        extension_bits | 3,
        (u8::from(w) << 7) | (encoded_vvvv << 3) | (u8::from(l) << 2) | 1,
        opcode,
        modrm,
        immediate,
    ]
}

#[test]
fn classifier_covers_all_65_536_prefix_opcode_and_modrm_register_shapes() {
    let mut classified = 0_usize;
    for opcode in [0x48, 0x49] {
        for extension_bits in (0_u8..8).map(|value| value << 5) {
            for w in [false, true] {
                for encoded_vvvv in 0_u8..16 {
                    for l in [false, true] {
                        for reg_rm in 0_u8..=0x3F {
                            let immediate = reg_rm
                                .wrapping_mul(17)
                                .wrapping_add(encoded_vvvv)
                                .wrapping_add(opcode);
                            let bytes = encoding(
                                extension_bits,
                                w,
                                encoded_vvvv,
                                l,
                                opcode,
                                0xC0 | reg_rm,
                                immediate,
                            );
                            assert!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .is_vex_register_vpermil2(),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 65_536);

    for immediate in u8::MIN..=u8::MAX {
        let bytes = encoding(
            0x40,
            immediate & 1 != 0,
            immediate >> 4,
            immediate & 2 != 0,
            if immediate & 4 == 0 { 0x48 } else { 0x49 },
            0xCB,
            immediate,
        );
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_vpermil2(),
            "{bytes:02X?}"
        );
    }

    assert_eq!(
        X86InstructionBytes::new(&[0xC4, 0xE3, 0x69, 0x48, 0xCC, 0x30])
            .unwrap()
            .vex_vpermil2_destination_index(),
        Some(1)
    );
    assert_eq!(
        X86InstructionBytes::new(&[0xC4, 0x43, 0xED, 0x49, 0xCC, 0xBF])
            .unwrap()
            .vex_vpermil2_destination_index(),
        Some(9)
    );
}

#[test]
fn classifier_rejects_every_structural_frontier_and_all_memory_mod_values() {
    let canonical = encoding(0xE0, true, 0x0D, true, 0x48, 0xCA, 0xBD);
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
        [
            0x62,
            canonical[1],
            canonical[2],
            canonical[3],
            canonical[4],
            canonical[5],
        ]
        .to_vec(),
    ];
    for (index, value) in [
        (1, (canonical[1] & !0x1F) | 1),
        (1, (canonical[1] & !0x1F) | 2),
        (1, (canonical[1] & !0x1F) | 4),
        (1, canonical[1] & !0x1F),
        (2, canonical[2] & !0x03),
        (2, (canonical[2] & !0x03) | 2),
        (2, (canonical[2] & !0x03) | 3),
        (3, 0x47),
        (3, 0x4A),
        (3, 0x68),
        (4, canonical[4] & 0x3F),
        (4, (canonical[4] & 0x3F) | 0x40),
        (4, (canonical[4] & 0x3F) | 0x80),
    ] {
        let mut bytes = canonical;
        bytes[index] = value;
        invalid.push(bytes.to_vec());
    }
    for bytes in invalid {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert!(!instruction.is_vex_register_vpermil2(), "{bytes:02X?}");
        assert_eq!(instruction.vex_vpermil2_destination_index(), None);
    }

    for p1 in u8::MIN..=u8::MAX {
        let mut bytes = canonical;
        bytes[2] = p1;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_vpermil2(),
            p1 & 0x03 == 1,
            "{bytes:02X?}"
        );
    }
    for immediate in u8::MIN..=u8::MAX {
        let mut bytes = canonical;
        bytes[5] = immediate;
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_vpermil2(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn dedicated_and_aggregate_spans_require_exact_contiguous_provenance() {
    let pc = 0x5E1E_C702;
    let instruction =
        X86InstructionBytes::new(&encoding(0x40, false, 3, true, 0x49, 0xFF, 0xCF)).unwrap();
    let mut block = SmirBlock::new(BlockId(43), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
    let provenance = std::collections::HashMap::from([((block.id, pc), instruction)]);

    for spans in [
        x86_vex_vpermil2_replay_spans(&block, &provenance),
        x86_native_replay_spans(&block, &provenance),
    ] {
        let span = spans.get(&0).expect("exact VPERMIL2 replay span");
        assert_eq!(span.end, 2);
        assert_eq!(span.instruction, instruction);
        assert!(!span.needs_avx512vl);
        assert!(!span.needs_avx512dq);
        assert!(!span.needs_avx512fp16);
        assert!(!span.preserve_mxcsr_de);
    }
    assert!(x86_vex_fma4_replay_spans(&block, &provenance).is_empty());
    assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());

    block.push_op(SmirOp::new(OpId(2), pc + 6, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(3), pc, OpKind::Nop));
    assert!(x86_native_replay_spans(&block, &provenance).is_empty());
}
