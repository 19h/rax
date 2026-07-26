//! Exact classifiers for operandless VEX `VZEROUPPER`/`VZEROALL` replay.

use super::*;

fn expected(bytes: &[u8]) -> Option<bool> {
    match bytes {
        &[0xC5, p1, 0x77] if p1 & 0x7B == 0x78 => Some(p1 & 0x04 != 0),
        &[0xC4, p0, p1, 0x77] if p0 & 0x1F == 1 && p1 & 0x7B == 0x78 => Some(p1 & 0x04 != 0),
        _ => None,
    }
}

#[test]
fn classifier_exhausts_two_and_three_byte_vex_prefix_frontiers() {
    let mut classified = 0usize;

    for p1 in u8::MIN..=u8::MAX {
        let bytes = [0xC5, p1, 0x77];
        let actual = X86InstructionBytes::new(&bytes)
            .unwrap()
            .vex_zeroes_all_register_bits();
        assert_eq!(actual, expected(&bytes), "{bytes:02X?}");
        classified += usize::from(actual.is_some());
    }

    for p0 in u8::MIN..=u8::MAX {
        for p1 in u8::MIN..=u8::MAX {
            let bytes = [0xC4, p0, p1, 0x77];
            let actual = X86InstructionBytes::new(&bytes)
                .unwrap()
                .vex_zeroes_all_register_bits();
            assert_eq!(actual, expected(&bytes), "{bytes:02X?}");
            classified += usize::from(actual.is_some());
        }
    }

    // C5 contributes 2 encodings per instruction (R' ignored); C4 contributes
    // 16 (R'/X'/B' and W ignored): 2 * (2 + 16) = 36.
    assert_eq!(classified, 36);
}

#[test]
fn classifier_accepts_independent_llvm_wig_and_extension_samples() {
    // LLVM 23 independently decoded every sample below to the canonical
    // mnemonic shown by VEX.L, including every ignored R'/X'/B'/W mutation.
    for (bytes, zero_all) in [
        (&[0xC5, 0xF8, 0x77][..], false),
        (&[0xC5, 0x78, 0x77], false),
        (&[0xC5, 0xFC, 0x77], true),
        (&[0xC5, 0x7C, 0x77], true),
        (&[0xC4, 0xE1, 0x78, 0x77], false),
        (&[0xC4, 0x61, 0x78, 0x77], false),
        (&[0xC4, 0xA1, 0x78, 0x77], false),
        (&[0xC4, 0xC1, 0x78, 0x77], false),
        (&[0xC4, 0x21, 0x78, 0x77], false),
        (&[0xC4, 0x01, 0x78, 0x77], false),
        (&[0xC4, 0xE1, 0xF8, 0x77], false),
        (&[0xC4, 0x01, 0xF8, 0x77], false),
        (&[0xC4, 0xE1, 0x7C, 0x77], true),
        (&[0xC4, 0x01, 0xFC, 0x77], true),
    ] {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_zeroes_all_register_bits(),
            Some(zero_all),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn classifier_rejects_reserved_neighbors_and_nonexact_shapes() {
    let invalid: &[&[u8]] = &[
        &[0x0F, 0x77],                   // EMMS, not VEX
        &[0xC5, 0xF9, 0x77],             // mandatory 66
        &[0xC5, 0xFA, 0x77],             // mandatory F3
        &[0xC5, 0xE8, 0x77],             // nonreserved VEX.vvvv
        &[0xC5, 0xF8, 0x76],             // unrelated opcode
        &[0xC5, 0xF8, 0x77, 0xC0],       // unexpected ModR/M
        &[0xC4, 0xE2, 0x78, 0x77],       // map 0F38
        &[0xC4, 0xE3, 0x78, 0x77],       // map 0F3A
        &[0xC4, 0xE1, 0x79, 0x77],       // mandatory 66
        &[0xC4, 0xE1, 0x68, 0x77],       // nonreserved VEX.vvvv
        &[0xC4, 0xE1, 0x78],             // missing opcode
        &[0xC4, 0xE1, 0x78, 0x77, 0x00], // trailing byte
        &[0x62, 0xF1, 0x7C, 0x08, 0x77], // EVEX has no VZERO form
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_zeroes_all_register_bits(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_span_covers_the_complete_contiguous_semantic_group() {
    let pc = 0x4270;
    let mut block = SmirBlock::new(BlockId(17), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(1), pc, OpKind::Nop));

    for bytes in [&[0xC5, 0xF8, 0x77][..], &[0xC4, 0x01, 0xFC, 0x77]] {
        let instruction = X86InstructionBytes::new(bytes).unwrap();
        let provenance = HashMap::from([((BlockId(17), pc), instruction)]);
        for spans in [
            x86_vex_zero_replay_spans(&block, &provenance),
            x86_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 2, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert!(!span.needs_avx512vl, "{bytes:02X?}");
            assert!(!span.needs_avx512dq, "{bytes:02X?}");
            assert!(!span.needs_avx512fp16, "{bytes:02X?}");
        }
        assert!(
            x86_evex_native_replay_spans(&block, &provenance).is_empty(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_span_rejects_missing_and_noncontiguous_provenance() {
    let pc = 0x4270;
    let mut block = SmirBlock::new(BlockId(17), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(1), pc + 1, OpKind::Nop));
    block.push_op(SmirOp::new(OpId(2), pc, OpKind::Nop));

    let instruction = X86InstructionBytes::new(&[0xC5, 0xF8, 0x77]).unwrap();
    let provenance = HashMap::from([((BlockId(17), pc), instruction)]);
    assert!(x86_vex_zero_replay_spans(&block, &provenance).is_empty());
    assert!(x86_native_replay_spans(&block, &provenance).is_empty());

    let mut contiguous = SmirBlock::new(BlockId(18), pc);
    contiguous.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));
    assert!(x86_vex_zero_replay_spans(&contiguous, &HashMap::new()).is_empty());
}
