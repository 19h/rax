//! Exact byte-classifier coverage for legacy SSE4.2 packed-string replay.

use super::*;
use crate::smir::ir::ops::X86PackedStringKind;
use crate::smir::ir::types::{ArchReg, OpWidth, X86Reg};

fn encoding(opcode: u8, rex: Option<u8>, modrm: u8, immediate: u8) -> Vec<u8> {
    assert!(matches!(opcode, 0x60..=0x63));
    assert!(rex.is_none_or(|byte| (0x40..=0x4F).contains(&byte)));
    let mut bytes = vec![0x66];
    bytes.extend(rex);
    bytes.extend([0x0F, 0x3A, opcode, modrm, immediate]);
    bytes
}

#[test]
fn classifier_accepts_all_1_114_112_canonical_register_encodings() {
    let mut classified = 0usize;
    for opcode in 0x60..=0x63 {
        for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
            for modrm in 0xC0..=0xFF {
                for immediate in u8::MIN..=u8::MAX {
                    let bytes = encoding(opcode, rex, modrm, immediate);
                    let instruction = X86InstructionBytes::new(&bytes).unwrap();
                    assert!(
                        instruction.is_legacy_register_packed_string_compare(),
                        "{bytes:02X?}"
                    );
                    assert!(!instruction.is_vex_register_packed_string_compare());
                    classified += 1;
                }
            }
        }
    }
    assert_eq!(classified, 4 * 17 * 64 * 256);
}

#[test]
fn classifier_rejects_every_structural_frontier() {
    let canonical = encoding(0x60, Some(0x4F), 0xCA, 0xA5);
    let mut invalid = vec![
        canonical[..canonical.len() - 1].to_vec(),
        canonical.iter().copied().chain([0]).collect(),
        canonical[1..].to_vec(),
        [0x4F, 0x66, 0x0F, 0x3A, 0x60, 0xCA, 0xA5].to_vec(),
        [0x66, 0x66, 0x0F, 0x3A, 0x60, 0xCA, 0xA5].to_vec(),
        [0xF2, 0x66, 0x0F, 0x3A, 0x60, 0xCA, 0xA5].to_vec(),
        [0x66, 0xD5, 0x00, 0x0F, 0x3A, 0x60, 0xCA, 0xA5].to_vec(),
        [0xC4, 0xE3, 0x79, 0x60, 0xCA, 0xA5].to_vec(),
    ];
    for (index, value) in [
        (2, 0x0E),                // neighboring escape map
        (3, 0x38),                // neighboring opcode map
        (4, 0x5F),                // neighboring opcode
        (4, 0x64),                // neighboring opcode
        (5, canonical[5] & 0x3F), // memory source
    ] {
        let mut bytes = canonical.clone();
        bytes[index] = value;
        invalid.push(bytes);
    }
    for bytes in invalid {
        assert!(
            !X86InstructionBytes::new(&bytes)
                .unwrap()
                .is_legacy_register_packed_string_compare(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn dedicated_and_aggregate_spans_require_the_exact_architectural_operation() {
    let pc = 0x5043_4D50;
    let instruction = X86InstructionBytes::new(&encoding(0x61, Some(0x4D), 0xCA, 0xA5)).unwrap();
    let mut block = SmirBlock::new(BlockId(0), pc);
    block.push_op(SmirOp::new(
        OpId(0),
        pc,
        OpKind::X86PackedStringCompare {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
            len1: Some(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
            len2: Some(VReg::Arch(ArchReg::X86(X86Reg::Rdx))),
            length_width: OpWidth::W64,
            kind: X86PackedStringKind::ExplicitIndex,
            imm: 0xA5,
            zero_upper: false,
        },
    ));
    let provenance = std::collections::HashMap::from([((block.id, pc), instruction)]);
    for spans in [
        x86_legacy_packed_string_replay_spans(&block, &provenance),
        x86_native_replay_spans(&block, &provenance),
    ] {
        assert_eq!(spans.get(&0).map(|span| span.end), Some(1));
        assert_eq!(
            spans.get(&0).map(|span| span.instruction),
            Some(instruction)
        );
    }

    let OpKind::X86PackedStringCompare { imm, .. } = &mut block.ops[0].kind else {
        unreachable!()
    };
    *imm ^= 1;
    assert!(x86_legacy_packed_string_replay_spans(&block, &provenance).is_empty());
    assert!(x86_native_replay_spans(&block, &provenance).is_empty());
}
