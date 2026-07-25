//! Exact classifier and span tests for AVX VEX variable blends.

use super::*;

const OPCODES: [u8; 3] = [0x4A, 0x4B, 0x4C];

fn encoding(
    extension_bits: u8,
    w: bool,
    encoded_vvvv: u8,
    l: bool,
    opcode: u8,
    modrm: u8,
    is4: u8,
) -> [u8; 6] {
    assert_eq!(extension_bits & !0xE0, 0);
    assert!(encoded_vvvv < 16);
    [
        0xC4,
        extension_bits | 3,
        (u8::from(w) << 7) | (encoded_vvvv << 3) | (u8::from(l) << 2) | 1,
        opcode,
        modrm,
        is4,
    ]
}

fn expected_requirement(opcode: u8, w: bool, l: bool) -> Option<bool> {
    if w {
        return None;
    }
    match opcode {
        0x4A | 0x4B => Some(false),
        0x4C => Some(l),
        _ => None,
    }
}

#[test]
fn classifier_exhaustively_covers_98_304_prefix_opcode_and_register_combinations() {
    let mut accepted = 0usize;
    let mut tested = 0usize;
    for opcode in OPCODES {
        for extension_bits in (0u8..8).map(|value| value << 5) {
            for w in [false, true] {
                for encoded_vvvv in 0u8..16 {
                    for l in [false, true] {
                        for reg_rm in 0u8..=0x3F {
                            let is4 = (((encoded_vvvv ^ reg_rm) & 0x0F) << 4)
                                | (opcode.wrapping_add(reg_rm) & 0x0F);
                            let bytes = encoding(
                                extension_bits,
                                w,
                                encoded_vvvv,
                                l,
                                opcode,
                                0xC0 | reg_rm,
                                is4,
                            );
                            let expected = expected_requirement(opcode, w, l);
                            assert_eq!(
                                X86InstructionBytes::new(&bytes)
                                    .unwrap()
                                    .vex_register_variable_blend_needs_avx2(),
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
    assert_eq!(tested, 98_304);

    // imm8[7:4] selects every architectural mask register; imm8[3:0] is
    // ignored. Exercise every combination independently of the prefix sweep.
    for is4 in u8::MIN..=u8::MAX {
        let opcode = OPCODES[usize::from(is4) % OPCODES.len()];
        let l = is4 & 1 != 0;
        let bytes = encoding(0x40, false, is4 >> 4, l, opcode, 0xCB, is4);
        assert_eq!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_register_variable_blend_needs_avx2(),
            expected_requirement(opcode, false, l),
            "{bytes:02X?}"
        );
    }

    // Independently assembled by LLVM 23.
    for (bytes, needs_avx2) in [
        ([0xC4, 0xE3, 0x61, 0x4A, 0xCA, 0x40], false),
        ([0xC4, 0x43, 0x31, 0x4B, 0xDA, 0x80], false),
        ([0xC4, 0xE3, 0x61, 0x4C, 0xCA, 0x40], false),
        ([0xC4, 0x43, 0x0D, 0x4C, 0xE5, 0xF0], true),
    ] {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            instruction.vex_register_variable_blend_needs_avx2(),
            Some(needs_avx2),
            "{bytes:02X?}"
        );
    }
    assert_eq!(
        X86InstructionBytes::new(&[0xC4, 0xE3, 0x61, 0x4A, 0xCA, 0x40])
            .unwrap()
            .vex_variable_blend_destination_index(),
        Some(1)
    );
    assert_eq!(
        X86InstructionBytes::new(&[0xC4, 0x43, 0x31, 0x4B, 0xDA, 0x80])
            .unwrap()
            .vex_variable_blend_destination_index(),
        Some(11)
    );
    assert_eq!(
        X86InstructionBytes::new(&[0xC4, 0x43, 0x0D, 0x4C, 0xE5, 0xF0])
            .unwrap()
            .vex_variable_blend_destination_index(),
        Some(12)
    );
}

#[test]
fn classifier_rejects_every_structural_and_reserved_frontier() {
    let canonical = encoding(0xE0, false, 0x0D, true, 0x4C, 0xCA, 0x4F);
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
        (3, 0x49),
        (3, 0x4D),
        (3, 0x02),
        (3, 0x0E),
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

    for bytes in invalid {
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        assert_eq!(
            instruction.vex_register_variable_blend_needs_avx2(),
            None,
            "{bytes:02X?}"
        );
        assert_eq!(instruction.vex_variable_blend_destination_index(), None);
    }
}

#[test]
fn dedicated_and_aggregate_spans_require_exact_contiguous_provenance() {
    let pc = 0xB14D;
    let instruction =
        X86InstructionBytes::new(&encoding(0x40, false, 3, true, 0x4A, 0xFF, 0x96)).unwrap();
    let mut block = SmirBlock::new(BlockId(41), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));
    let provenance = HashMap::from([((block.id, pc), instruction)]);

    for spans in [
        x86_vex_variable_blend_replay_spans(&block, &provenance),
        x86_native_replay_spans(&block, &provenance),
    ] {
        let span = spans.get(&0).expect("exact VEX variable-blend span");
        assert_eq!(span.end, 2);
        assert_eq!(span.instruction, instruction);
        assert!(!span.needs_avx512vl);
        assert!(!span.needs_avx512dq);
        assert!(!span.needs_avx512fp16);
        assert!(!span.preserve_mxcsr_de);
    }
    assert!(x86_vex_immediate_blend_replay_spans(&block, &provenance).is_empty());
    assert!(x86_evex_native_replay_spans(&block, &provenance).is_empty());

    block.push_op(SmirOp::new(OpId(2), pc + 6, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(3), pc, OpKind::Nop));
    assert!(x86_native_replay_spans(&block, &provenance).is_empty());
}
