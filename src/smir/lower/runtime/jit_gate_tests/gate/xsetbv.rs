//! XGETBV/XSETBV native-admission tests.

use super::*;
use crate::smir::ir::ops::SmirOp;
use crate::smir::ir::types::{OpId, SourceArch};
use crate::smir::ir::{SmirFunction, TrapKind, X86InstructionBytes};
use crate::smir::lift::SmirLifter;
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lower::runtime::jit_gate_tests::*;
use crate::smir::lower::runtime::*;

const XSETBV_PC: u64 = 0x1000;
const IGNORED_XSETBV_PREFIXES: [u8; 23] = [
    0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x67, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
    0x49, 0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F,
];

fn xsetbv_kind() -> OpKind {
    OpKind::X86XSetBv {
        selector: x86(X86Reg::Rcx),
        src_low: x86(X86Reg::Rax),
        src_high: x86(X86Reg::Rdx),
    }
}

fn xsetbv_function(source: Option<&[u8]>, following_pc: Option<u64>) -> SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), XSETBV_PC);
    builder.push_op(XSETBV_PC, xsetbv_kind());
    if let Some(pc) = following_pc {
        builder.push_op(pc, OpKind::Nop);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    if let Some(source) = source {
        let block_id = function.blocks[0].id;
        function.x86_instruction_bytes.insert(
            (block_id, XSETBV_PC),
            X86InstructionBytes::new(source).expect("bounded nonempty test source"),
        );
    }
    function
}

fn xsetbv_source(prefixes: &[u8]) -> Vec<u8> {
    let mut source = prefixes.to_vec();
    source.extend_from_slice(&[0x0F, 0x01, 0xD1]);
    source
}

fn source_lifts_as_exact_xsetbv(source: &[u8]) -> bool {
    let mut lifter = X86_64Lifter::strict();
    let mut context = crate::smir::lift::LiftContext::new(SourceArch::X86_64);
    lifter
        .lift_insn(XSETBV_PC, source, &mut context)
        .ok()
        .is_some_and(|result| {
            result.bytes_consumed == source.len()
                && matches!(
                    result.ops.as_slice(),
                    [SmirOp {
                        kind: OpKind::X86XSetBv {
                            selector: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
                            src_low: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                            src_high: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
                        },
                        x86_hint: None,
                        ..
                    }]
                )
        })
}

#[test]
fn x86_xgetbv_gate_admits_only_fixed_implicit_register_shape() {
    let valid = OpKind::X86XGetBv {
        dst_low: x86(X86Reg::Rax),
        dst_high: x86(X86Reg::Rdx),
        selector: x86(X86Reg::Rcx),
    };
    assert!(valid.is_jit_safe(), "XGETBV must be class-whitelisted");
    assert!(
        x86_gate(valid),
        "architectural XGETBV must enter native tier"
    );

    for malformed in [
        OpKind::X86XGetBv {
            dst_low: x86(X86Reg::R8),
            dst_high: x86(X86Reg::Rdx),
            selector: x86(X86Reg::Rcx),
        },
        OpKind::X86XGetBv {
            dst_low: x86(X86Reg::Rax),
            dst_high: x86(X86Reg::R9),
            selector: x86(X86Reg::Rcx),
        },
        OpKind::X86XGetBv {
            dst_low: x86(X86Reg::Rax),
            dst_high: x86(X86Reg::Rdx),
            selector: x86(X86Reg::R10),
        },
    ] {
        assert!(malformed.is_jit_safe());
        assert!(!x86_gate(malformed), "malformed XGETBV must deopt");
    }
}

#[test]
fn x86_xsetbv_gate_matches_the_lifters_complete_ignored_prefix_grammar() {
    let bare = xsetbv_source(&[]);
    assert!(source_lifts_as_exact_xsetbv(&bare));
    assert!(is_native_clobber_safe(&xsetbv_function(Some(&bare), None)));

    // Exhaust every possible single leading byte. Admission must agree with
    // an exact one-instruction lift, not merely with a hand-maintained list.
    for prefix in 0u8..=u8::MAX {
        let source = xsetbv_source(&[prefix]);
        assert_eq!(
            is_native_clobber_safe(&xsetbv_function(Some(&source), None)),
            source_lifts_as_exact_xsetbv(&source),
            "single prefix {prefix:02X}"
        );
    }

    // Prefix decoding is iterative. Cover every two-byte combination in the
    // accepted alphabet, including duplicate, reordered, and non-final REX.
    for first in IGNORED_XSETBV_PREFIXES {
        for second in IGNORED_XSETBV_PREFIXES {
            let source = xsetbv_source(&[first, second]);
            assert!(source_lifts_as_exact_xsetbv(&source));
            assert!(
                is_native_clobber_safe(&xsetbv_function(Some(&source), None)),
                "ignored prefixes {first:02X} {second:02X}"
            );
        }
    }

    // Twelve prefixes plus the three-byte opcode is the architectural maximum.
    let maximal = xsetbv_source(&[0x67; 12]);
    assert_eq!(maximal.len(), 15);
    assert!(source_lifts_as_exact_xsetbv(&maximal));
    assert!(is_native_clobber_safe(&xsetbv_function(
        Some(&maximal),
        None
    )));
    assert!(X86InstructionBytes::new(&[0x67; 16]).is_none());
}

#[test]
fn x86_xsetbv_gate_uses_exact_source_boundary_and_fails_closed() {
    assert!(
        xsetbv_kind().is_jit_safe(),
        "XSETBV must be class-whitelisted"
    );
    assert!(
        !is_native_clobber_safe(&xsetbv_function(None, None)),
        "missing provenance"
    );

    for source in [
        &[0x0F, 0x01][..],
        &[0x0F, 0x01, 0xD0][..],
        &[0x0F, 0x01, 0xD2][..],
        &[0x0F, 0x01, 0xD1, 0x90][..],
        &[0x66, 0x0F, 0x01, 0xD1][..],
        &[0xF2, 0x0F, 0x01, 0xD1][..],
        &[0xF3, 0x0F, 0x01, 0xD1][..],
        &[0xF0, 0x0F, 0x01, 0xD1][..],
        &[0xD5, 0x00, 0x0F, 0x01, 0xD1][..],
    ] {
        assert!(
            !source_lifts_as_exact_xsetbv(source),
            "source {source:02X?}"
        );
        assert!(
            !is_native_clobber_safe(&xsetbv_function(Some(source), None)),
            "source {source:02X?}"
        );
    }

    let bare = xsetbv_source(&[]);
    assert!(
        is_native_clobber_safe(&xsetbv_function(Some(&bare), Some(XSETBV_PC + 3))),
        "matching explicit next PC"
    );
    assert!(
        !is_native_clobber_safe(&xsetbv_function(Some(&bare), Some(XSETBV_PC + 4))),
        "mismatched explicit next PC"
    );
    let prefixed = xsetbv_source(&[0x67]);
    assert!(is_native_clobber_safe(&xsetbv_function(
        Some(&prefixed),
        Some(XSETBV_PC + 4)
    )));

    let mut wrong_key = xsetbv_function(Some(&bare), None);
    let instruction = wrong_key
        .x86_instruction_bytes
        .remove(&(BlockId(0), XSETBV_PC))
        .unwrap();
    wrong_key
        .x86_instruction_bytes
        .insert((BlockId(0), XSETBV_PC + 1), instruction);
    assert!(!is_native_clobber_safe(&wrong_key), "wrong provenance key");

    let mut hinted = xsetbv_function(Some(&bare), None);
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::LegacyHighByteReg);
    assert!(!is_native_clobber_safe(&hinted), "unexpected semantic hint");

    let mut duplicate_group = xsetbv_function(Some(&bare), None);
    duplicate_group.blocks[0]
        .ops
        .push(SmirOp::new(OpId(1), XSETBV_PC, OpKind::Nop));
    assert!(
        !is_native_clobber_safe(&duplicate_group),
        "XSETBV provenance denotes exactly one same-PC semantic op"
    );

    for malformed in [
        OpKind::X86XSetBv {
            selector: x86(X86Reg::R8),
            src_low: x86(X86Reg::Rax),
            src_high: x86(X86Reg::Rdx),
        },
        OpKind::X86XSetBv {
            selector: x86(X86Reg::Rcx),
            src_low: x86(X86Reg::R9),
            src_high: x86(X86Reg::Rdx),
        },
        OpKind::X86XSetBv {
            selector: x86(X86Reg::Rcx),
            src_low: x86(X86Reg::Rax),
            src_high: x86(X86Reg::R10),
        },
    ] {
        let mut function = xsetbv_function(Some(&bare), None);
        function.blocks[0].ops[0].kind = malformed;
        assert!(!is_native_clobber_safe(&function), "malformed XSETBV");
    }

    let overflow_pc = u64::MAX - 2;
    let mut overflow = xsetbv_function(Some(&bare), None);
    overflow.blocks[0].guest_pc = overflow_pc;
    overflow.blocks[0].ops[0].guest_pc = overflow_pc;
    overflow.x86_instruction_bytes.clear();
    overflow.x86_instruction_bytes.insert(
        (overflow.blocks[0].id, overflow_pc),
        X86InstructionBytes::new(&bare).unwrap(),
    );
    assert!(!is_native_clobber_safe(&overflow), "resume-PC overflow");
}

#[test]
fn x86_xsetbv_gate_checks_only_the_reachable_native_prefix() {
    let source = xsetbv_source(&[]);
    let mut builder = FunctionBuilder::new(FunctionId(0), XSETBV_PC);
    let unreachable = builder.alloc_vreg();
    builder.push_op(XSETBV_PC, xsetbv_kind());
    builder.push_op(
        XSETBV_PC + 3,
        OpKind::Mov {
            dst: unreachable,
            src: SrcOperand::Imm(0xDEAD),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::InvalidOpcode,
    });
    let mut function = builder.finish();
    function.x86_instruction_bytes.insert(
        (function.blocks[0].id, XSETBV_PC),
        X86InstructionBytes::new(&source).unwrap(),
    );
    assert!(
        is_native_clobber_safe(&function),
        "XSETBV exits before an unsafe virtual destination and trap"
    );

    let mut builder = FunctionBuilder::new(FunctionId(1), XSETBV_PC - 1);
    let reachable = builder.alloc_vreg();
    builder.push_op(
        XSETBV_PC - 1,
        OpKind::Mov {
            dst: reachable,
            src: SrcOperand::Imm(0xBEEF),
            width: OpWidth::W64,
        },
    );
    builder.push_op(XSETBV_PC, xsetbv_kind());
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.x86_instruction_bytes.insert(
        (function.blocks[0].id, XSETBV_PC),
        X86InstructionBytes::new(&source).unwrap(),
    );
    assert!(
        !is_native_clobber_safe(&function),
        "unsafe operations before XSETBV remain reachable"
    );
}
