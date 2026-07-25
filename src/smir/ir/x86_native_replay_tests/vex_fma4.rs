//! Exact classifier and span tests for AMD AVX VEX FMA4 replay.

use super::*;

const OPCODES: [u8; 20] = [
    0x5C, 0x5D, 0x5E, 0x5F, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, 0x78, 0x79, 0x7A, 0x7B,
    0x7C, 0x7D, 0x7E, 0x7F,
];

fn encoding(
    extension_bits: u8,
    w: bool,
    encoded_vvvv: u8,
    l: bool,
    opcode: u8,
    modrm: u8,
    is4: u8,
    low: u8,
) -> [u8; 6] {
    assert_eq!(extension_bits & !0xE0, 0);
    assert!(encoded_vvvv < 16 && is4 < 16 && low < 16);
    [
        0xC4,
        extension_bits | 3,
        (u8::from(w) << 7) | (encoded_vvvv << 3) | (u8::from(l) << 2) | 1,
        opcode,
        modrm,
        (is4 << 4) | low,
    ]
}

#[test]
fn classifier_covers_all_655_360_prefix_opcode_modrm_combinations_and_every_is4_byte() {
    let mut classified = 0usize;
    for opcode in OPCODES {
        for extension_bits in (0u8..8).map(|value| value << 5) {
            for w in [false, true] {
                for encoded_vvvv in 0u8..16 {
                    for l in [false, true] {
                        for reg_rm in 0u8..=0x3F {
                            let is4 = reg_rm.wrapping_add(opcode) & 0x0F;
                            let low = reg_rm.wrapping_add(encoded_vvvv) & 0x0F;
                            let bytes = encoding(
                                extension_bits,
                                w,
                                encoded_vvvv,
                                l,
                                opcode,
                                0xC0 | reg_rm,
                                is4,
                                low,
                            );
                            assert!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .is_vex_register_fma4(),
                                "{bytes:02X?}"
                            );
                            classified += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(classified, 655_360);

    for immediate in u8::MIN..=u8::MAX {
        let bytes = encoding(
            0x40,
            immediate & 1 != 0,
            immediate >> 4,
            immediate & 2 != 0,
            OPCODES[usize::from(immediate) % OPCODES.len()],
            0xCB,
            immediate >> 4,
            immediate & 0x0F,
        );
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_fma4(),
            "{bytes:02X?}"
        );
    }

    // Independently assembled by LLVM 23.
    for bytes in [
        [0xC4, 0xE3, 0xE9, 0x68, 0xCC, 0x30], // vfmaddps xmm1,xmm2,xmm3,xmm4
        [0xC4, 0x43, 0xAD, 0x68, 0xCC, 0xB0], // vfmaddps ymm9,ymm10,ymm11,ymm12
        [0xC4, 0xE3, 0xE9, 0x7F, 0xCC, 0x30], // vfnmsubsd xmm1,xmm2,xmm3,xmm4
    ] {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_fma4(),
            "{bytes:02X?}"
        );
    }
    assert_eq!(
        X86InstructionBytes::new(&[0xC4, 0xE3, 0xE9, 0x68, 0xCC, 0x30])
            .unwrap()
            .vex_fma4_destination_index(),
        Some(1)
    );
    assert_eq!(
        X86InstructionBytes::new(&[0xC4, 0x43, 0xAD, 0x68, 0xCC, 0xB0])
            .unwrap()
            .vex_fma4_destination_index(),
        Some(9)
    );
}

#[test]
fn classifier_rejects_every_structural_frontier_but_accepts_w_l_and_is4_low_bits() {
    let canonical = encoding(0xE0, true, 0x0D, true, 0x68, 0xCA, 0x0B, 0x0D);
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
        (1, (canonical[1] & !0x1F) | 1),   // map 0F
        (1, (canonical[1] & !0x1F) | 2),   // map 0F38
        (1, (canonical[1] & !0x1F) | 4),   // another map
        (1, canonical[1] & !0x1F),         // reserved map zero
        (2, canonical[2] & !0x03),         // no mandatory prefix
        (2, (canonical[2] & !0x03) | 2),   // F3 instead of 66
        (2, (canonical[2] & !0x03) | 3),   // F2 instead of 66
        (3, 0x5B),                         // below first range
        (3, 0x60),                         // gap after first range
        (3, 0x67),                         // below second range
        (3, 0x70),                         // gap after second range
        (3, 0x77),                         // below third range
        (3, 0x80),                         // above final range
        (4, canonical[4] & 0x3F),          // memory source, mod=00
        (4, (canonical[4] & 0x3F) | 0x40), // memory source, mod=01
        (4, (canonical[4] & 0x3F) | 0x80), // memory source, mod=10
    ] {
        let mut bytes = canonical;
        bytes[index] = value;
        invalid.push(bytes.to_vec());
    }
    for bytes in invalid {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert!(!instruction.is_vex_register_fma4(), "{bytes:02X?}");
        assert_eq!(instruction.vex_fma4_destination_index(), None);
    }

    for p1 in u8::MIN..=u8::MAX {
        let mut bytes = canonical;
        bytes[2] = p1;
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_vex_register_fma4(),
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
                .is_vex_register_fma4(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn dedicated_and_aggregate_spans_require_exact_contiguous_provenance() {
    let pc = 0xF4A4;
    let instruction =
        X86InstructionBytes::new(&encoding(0x40, false, 3, true, 0x7E, 0xFF, 12, 15)).unwrap();
    let mut block = SmirBlock::new(BlockId(39), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
    let provenance = std::collections::HashMap::from([((block.id, pc), instruction)]);

    for spans in [
        x86_vex_fma4_replay_spans(&block, &provenance),
        x86_native_replay_spans(&block, &provenance),
    ] {
        let span = spans.get(&0).expect("exact VEX FMA4 replay span");
        assert_eq!(span.end, 2);
        assert_eq!(span.instruction, instruction);
        assert!(!span.needs_avx512vl);
        assert!(!span.needs_avx512dq);
        assert!(!span.needs_avx512fp16);
        assert!(!span.preserve_mxcsr_de);
    }
    assert!(x86_vex_fma3_replay_spans(&block, &provenance).is_empty());
    assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());

    block.push_op(SmirOp::new(OpId(2), pc + 6, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(3), pc, OpKind::Nop));
    assert!(x86_native_replay_spans(&block, &provenance).is_empty());
}
