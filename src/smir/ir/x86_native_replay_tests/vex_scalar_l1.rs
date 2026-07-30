//! Deterministic canonicalization coverage for scalar VEX.L=1 byte images.

use super::*;
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};

const PC: u64 = 0xC410_1151;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarForm {
    name: &'static str,
    pp: u8,
    opcode: u8,
    immediate: Option<u8>,
    reserved_vvvv: bool,
}

const SCALAR_FORMS: [ScalarForm; 30] = [
    ScalarForm {
        name: "VADDSS",
        pp: 2,
        opcode: 0x58,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VADDSD",
        pp: 3,
        opcode: 0x58,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VMULSS",
        pp: 2,
        opcode: 0x59,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VMULSD",
        pp: 3,
        opcode: 0x59,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VSUBSS",
        pp: 2,
        opcode: 0x5C,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VSUBSD",
        pp: 3,
        opcode: 0x5C,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VMINSS",
        pp: 2,
        opcode: 0x5D,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VMINSD",
        pp: 3,
        opcode: 0x5D,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VDIVSS",
        pp: 2,
        opcode: 0x5E,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VDIVSD",
        pp: 3,
        opcode: 0x5E,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VMAXSS",
        pp: 2,
        opcode: 0x5F,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VMAXSD",
        pp: 3,
        opcode: 0x5F,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VCMPSS",
        pp: 2,
        opcode: 0xC2,
        immediate: Some(0x1F),
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VCMPSD",
        pp: 3,
        opcode: 0xC2,
        immediate: Some(0x1F),
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VUCOMISS",
        pp: 0,
        opcode: 0x2E,
        immediate: None,
        reserved_vvvv: true,
    },
    ScalarForm {
        name: "VUCOMISD",
        pp: 1,
        opcode: 0x2E,
        immediate: None,
        reserved_vvvv: true,
    },
    ScalarForm {
        name: "VCOMISS",
        pp: 0,
        opcode: 0x2F,
        immediate: None,
        reserved_vvvv: true,
    },
    ScalarForm {
        name: "VCOMISD",
        pp: 1,
        opcode: 0x2F,
        immediate: None,
        reserved_vvvv: true,
    },
    ScalarForm {
        name: "VCVTSI2SS",
        pp: 2,
        opcode: 0x2A,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VCVTSI2SD",
        pp: 3,
        opcode: 0x2A,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VCVTTSS2SI",
        pp: 2,
        opcode: 0x2C,
        immediate: None,
        reserved_vvvv: true,
    },
    ScalarForm {
        name: "VCVTTSD2SI",
        pp: 3,
        opcode: 0x2C,
        immediate: None,
        reserved_vvvv: true,
    },
    ScalarForm {
        name: "VCVTSS2SI",
        pp: 2,
        opcode: 0x2D,
        immediate: None,
        reserved_vvvv: true,
    },
    ScalarForm {
        name: "VCVTSD2SI",
        pp: 3,
        opcode: 0x2D,
        immediate: None,
        reserved_vvvv: true,
    },
    ScalarForm {
        name: "VCVTSS2SD",
        pp: 2,
        opcode: 0x5A,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VCVTSD2SS",
        pp: 3,
        opcode: 0x5A,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VSQRTSS",
        pp: 2,
        opcode: 0x51,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VSQRTSD",
        pp: 3,
        opcode: 0x51,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VMOVSS load",
        pp: 2,
        opcode: 0x10,
        immediate: None,
        reserved_vvvv: false,
    },
    ScalarForm {
        name: "VMOVSS store",
        pp: 2,
        opcode: 0x11,
        immediate: None,
        reserved_vvvv: true,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VexForm {
    C5,
    C4W0,
    C4W1,
}

impl VexForm {
    const ALL: [Self; 3] = [Self::C5, Self::C4W0, Self::C4W1];
}

fn encoding(form: ScalarForm, vex: VexForm, memory: bool, l: bool) -> Vec<u8> {
    let encoded_vvvv = if form.reserved_vvvv || (memory && matches!(form.opcode, 0x10 | 0x11)) {
        0x78
    } else {
        0x68
    };
    let p1 = encoded_vvvv
        | (if l { 0x04 } else { 0 })
        | form.pp
        | if vex == VexForm::C4W1 { 0x80 } else { 0 };
    let modrm = if memory { 0x01 } else { 0xC1 };
    let mut bytes = match vex {
        VexForm::C5 => vec![0xC5, 0x80 | (p1 & 0x7F), form.opcode, modrm],
        VexForm::C4W0 | VexForm::C4W1 => vec![0xC4, 0xE1, p1, form.opcode, modrm],
    };
    if let Some(immediate) = form.immediate {
        bytes.push(immediate);
    }
    bytes
}

fn lifted_semantics(bytes: &[u8]) -> (String, usize) {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    let semantics = result
        .ops
        .iter()
        .map(|op| format!("{:?}", op.kind))
        .collect::<Vec<_>>()
        .join("\n");
    (
        format!("{:?}\n{semantics}", result.control_flow),
        result.ops.len(),
    )
}

#[test]
fn all_180_register_and_memory_images_canonicalize_by_only_clearing_l() {
    let mut checked = 0usize;
    for form in SCALAR_FORMS {
        for vex in VexForm::ALL {
            for memory in [false, true] {
                let source = encoding(form, vex, memory, true);
                let expected = encoding(form, vex, memory, false);
                let source_instruction = X86InstructionBytes::new(&source).unwrap();
                let canonical = source_instruction
                    .vex_scalar_l1_canonical_l0()
                    .unwrap_or_else(|| panic!("{form:?} {vex:?} memory={memory}: {source:02X?}"));
                assert_eq!(
                    canonical.as_slice(),
                    expected,
                    "{form:?} {vex:?} memory={memory}"
                );
                assert_eq!(source.len(), expected.len());
                let differing = source
                    .iter()
                    .zip(&expected)
                    .enumerate()
                    .filter_map(|(index, (left, right))| (left != right).then_some(index))
                    .collect::<Vec<_>>();
                let p1 = if vex == VexForm::C5 { 1 } else { 2 };
                assert_eq!(differing, [p1], "{form:?} {vex:?} memory={memory}");
                assert_eq!(source[p1] ^ expected[p1], 0x04);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 30 * 3 * 2);
}

#[test]
fn all_180_l1_images_lift_to_the_exact_l0_semantic_graph() {
    let mut checked = 0usize;
    for form in SCALAR_FORMS {
        for vex in VexForm::ALL {
            for memory in [false, true] {
                let l0 = encoding(form, vex, memory, false);
                let l1 = encoding(form, vex, memory, true);
                assert_eq!(
                    lifted_semantics(&l1),
                    lifted_semantics(&l0),
                    "{form:?} {vex:?} memory={memory}"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 30 * 3 * 2);
}

#[test]
fn all_90_register_images_replay_the_canonical_l0_instruction() {
    let mut checked = 0usize;
    for form in SCALAR_FORMS {
        for vex in VexForm::ALL {
            let source = encoding(form, vex, false, true);
            let expected = X86InstructionBytes::new(&encoding(form, vex, false, false)).unwrap();
            let mut lifter = X86_64Lifter::strict();
            let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
            let result = lifter
                .lift_insn(PC, &source, &mut context)
                .unwrap_or_else(|error| panic!("{form:?} {vex:?}: {error:?}"));
            let mut block = SmirBlock::new(BlockId(0), PC);
            block.ops = result.ops;
            let provenance =
                HashMap::from([((block.id, PC), X86InstructionBytes::new(&source).unwrap())]);
            let spans = x86_native_replay_spans(&block, &provenance);
            let span = spans
                .get(&0)
                .unwrap_or_else(|| panic!("{form:?} {vex:?}: {source:02X?}"));
            assert_eq!(span.end, block.ops.len(), "{form:?} {vex:?}");
            assert_eq!(span.instruction, expected, "{form:?} {vex:?}");
            assert!(!span.needs_avx512vl, "{form:?} {vex:?}");
            assert!(!span.needs_avx512dq, "{form:?} {vex:?}");
            assert!(!span.needs_avx512fp16, "{form:?} {vex:?}");
            checked += 1;
        }
    }
    assert_eq!(checked, 30 * 3);
}

#[test]
fn canonicalizer_rejects_defined_width_reserved_and_nonexact_neighbors() {
    let invalid: &[&[u8]] = &[
        &[0xC5, 0xEA, 0x58, 0xC1],             // supported scalar L=0
        &[0xC5, 0xEC, 0x58, 0xC1],             // packed VADDPS VEX.256
        &[0xC5, 0xED, 0x58, 0xC1],             // packed VADDPD VEX.256
        &[0xC5, 0xEC, 0x51, 0xC1],             // packed VSQRTPS VEX.256
        &[0xC5, 0xED, 0xC2, 0xC1, 0x1F],       // packed VCMPPD VEX.256
        &[0xC5, 0xEF, 0x10, 0xC1],             // VMOVSD is VEX.LIG
        &[0xC5, 0xEF, 0x11, 0xC1],             // VMOVSD is VEX.LIG
        &[0xC5, 0x74, 0x2F, 0xC1],             // VCOMISS reserves VEX.vvvv
        &[0xC5, 0xEC, 0xC2, 0xC1, 0x20],       // VCMPSS predicate is > 31
        &[0xC4, 0xE2, 0x6E, 0x58, 0xC1],       // wrong VEX map
        &[0xC4, 0xE2, 0x6E, 0xA8, 0xC1],       // unrelated scalar FMA
        &[0x66, 0xC5, 0xEE, 0x58, 0xC1],       // forbidden mandatory prefix
        &[0xF0, 0xC5, 0xEE, 0x58, 0x01],       // forbidden LOCK prefix
        &[0x62, 0xF1, 0x6E, 0x08, 0x58, 0xC1], // EVEX neighbor
        &[0xC5, 0xEE, 0x58],                   // missing ModR/M
        &[0xC5, 0xEE, 0x58, 0x04],             // missing SIB
        &[0xC5, 0xEE, 0x58, 0xC1, 0x00],       // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .vex_scalar_l1_canonical_l0(),
            None,
            "{bytes:02X?}"
        );
    }

    for prefix in [0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x67] {
        let source = [prefix, 0xC5, 0xEE, 0x58, 0x01];
        let expected = [prefix, 0xC5, 0xEA, 0x58, 0x01];
        assert_eq!(
            X86InstructionBytes::new(&source)
                .unwrap()
                .vex_scalar_l1_canonical_l0()
                .map(|instruction| instruction.as_slice().to_vec()),
            Some(expected.to_vec()),
            "{source:02X?}"
        );
    }
}
