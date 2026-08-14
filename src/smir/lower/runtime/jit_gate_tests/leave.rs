//! Exact x86 LEAVE admission, provenance, optimizer, and lowerer coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{SmirOp, X86LeaveOp, X86LeaveWidth};
use crate::smir::ir::types::OpId;
use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    x86_jit_op_uses_mem_helper,
};
use crate::smir::lower::x86_64::{X86LeaveEncoding, x86_leave_encoding};
use crate::smir::lower::{
    SmirLowerer, X86_GUEST_CS_L_OFFSET, X86_GUEST_EFER_OFFSET, X86_GUEST_LOAD_FN_OFFSET,
    x86_64::X86_64Lowerer,
};
use crate::smir::optimize::{OptLevel, optimize_function};

const PC: u64 = 0x4C45_4100;
const SCANNER_PREFIXES: &[&[u8]] = &[
    &[],
    &[0x66],
    &[0xF2],
    &[0xF3],
    &[0x67],
    &[0x64],
    &[0x65],
    &[0x48],
    &[0x44],
    &[0x41],
    &[0x4D],
    &[0x66, 0x48],
    &[0xF2, 0x48],
    &[0xF3, 0x48],
];
const ACCEPTED_PREFIX_BYTES: &[u8] = &[
    0xF2, 0xF3, 0x2E, 0x36, 0x3E, 0x26, 0x64, 0x65, 0x66, 0x67, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45,
    0x46, 0x47, 0x48, 0x49, 0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F,
];

fn lift(bytes: &[u8]) -> SmirFunction {
    let mut context = LiftContext::new(crate::smir::SourceArch::X86_64);
    let result = X86_64Lifter::strict()
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(
        result.ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86Leave(..),
            ..
        }]
    ));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("complete LEAVE source"),
    );
    function
}

fn encoding(function: &SmirFunction) -> Option<X86LeaveEncoding> {
    x86_leave_encoding(&function.blocks[0], 0, &function.x86_instruction_bytes)
}

fn admitted(function: &SmirFunction, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(function, &HashMap::new(), allow_mem)
}

fn lower_with(
    function: &SmirFunction,
    mem_helpers: bool,
    fault_guards: bool,
) -> Result<Vec<u8>, crate::smir::lower::LowerError> {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    lowerer.lower_function(function)?;
    lowerer.finalize()
}

#[test]
fn leave_metadata_and_helper_requirements_are_exact() {
    let function = lift(&[0xC9]);
    let op = &function.blocks[0].ops[0];
    assert!(x86_jit_op_uses_mem_helper(&op.kind));
    assert!(!op.kind.is_jit_safe());
    assert!(!op.is_jit_safe());
    assert_eq!(
        op.kind.dests(),
        [
            VReg::Arch(ArchReg::X86(X86Reg::Rsp)),
            VReg::Arch(ArchReg::X86(X86Reg::Rbp)),
        ]
    );
    assert_eq!(
        op.kind.source_vregs(),
        [VReg::Arch(ArchReg::X86(X86Reg::Rbp))]
    );
    assert_eq!(op.kind.flags_read(), FlagSet::EMPTY);
    assert_eq!(op.kind.flags_written(), FlagSet::EMPTY);
    assert!(op.kind.has_side_effects());
    assert!(op.kind.reads_memory());
    assert!(!op.kind.writes_memory());
    assert!(!admitted(&function, false));
    assert!(admitted(&function, true));
    assert!(lower_with(&function, false, true).is_err());
    assert!(lower_with(&function, true, false).is_err());

    let code = lower_with(&function, true, true).expect("exact LEAVE must lower");
    for offset in [
        X86_GUEST_EFER_OFFSET,
        X86_GUEST_CS_L_OFFSET,
        X86_GUEST_LOAD_FN_OFFSET,
    ] {
        let offset = offset.to_le_bytes();
        assert!(
            code.windows(offset.len()).any(|window| window == offset),
            "missing GuestRegs offset {offset:02X?}"
        );
    }
}

#[test]
fn all_scanner_images_survive_optimization_and_lower_exactly() {
    for prefix in SCANNER_PREFIXES {
        let mut bytes = prefix.to_vec();
        bytes.push(0xC9);
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut function = lift(&bytes);
            optimize_function(&mut function, level);
            let decoded = encoding(&function)
                .unwrap_or_else(|| panic!("{bytes:02X?} {level:?}: missing exact encoding"));
            assert_eq!(decoded.next_pc, PC + bytes.len() as u64);
            assert!(admitted(&function, true), "{bytes:02X?} {level:?}");
            lower_with(&function, true, true).unwrap_or_else(|error| {
                panic!("{bytes:02X?} {level:?}: native lower failed: {error:?}")
            });
        }
    }
}

#[test]
fn every_accepted_legacy_prefix_pair_has_exact_provenance() {
    assert_eq!(ACCEPTED_PREFIX_BYTES.len(), 26);
    let mut images = 0usize;
    for first in ACCEPTED_PREFIX_BYTES {
        for second in ACCEPTED_PREFIX_BYTES {
            let bytes = [*first, *second, 0xC9];
            let function = lift(&bytes);
            assert!(encoding(&function).is_some(), "{bytes:02X?}");
            assert!(admitted(&function, true), "{bytes:02X?}");
            images += 1;
        }
    }
    assert_eq!(images, 26 * 26);
}

#[test]
fn all_rex2_payloads_agree_with_strict_map_selection() {
    for payload in u8::MIN..=u8::MAX {
        let bytes = [0xD5, payload, 0xC9];
        let mut context = LiftContext::new(crate::smir::SourceArch::X86_64);
        let result = X86_64Lifter::strict().lift_insn(PC, &bytes, &mut context);
        let is_leave = result.as_ref().is_ok_and(|lifted| {
            matches!(
                lifted.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::X86Leave(..),
                    ..
                }]
            )
        });
        assert_eq!(is_leave, payload & 0x80 == 0, "{bytes:02X?}: map selection");
        if is_leave {
            let function = lift(&bytes);
            let decoded = encoding(&function).expect("map-0 REX2 LEAVE encoding");
            assert!(decoded.requires_apx);
            assert_eq!(decoded.width, X86LeaveWidth::W64);
            assert!(admitted(&function, true));
        }
    }
}

#[test]
fn leave_rejects_missing_mismatched_and_cross_host_shapes() {
    let canonical = lift(&[0x66, 0xC9]);
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        &canonical,
        &HashMap::new()
    ));

    let mut malformed = Vec::new();
    let mut missing = canonical.clone();
    missing.x86_instruction_bytes.clear();
    malformed.push(("missing provenance", missing));

    let mut trailing = canonical.clone();
    trailing.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&[0x66, 0xC9, 0x90]).unwrap(),
    );
    malformed.push(("trailing source", trailing));

    let mut locked = canonical.clone();
    locked.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&[0xF0, 0xC9]).unwrap(),
    );
    malformed.push(("LOCK source", locked));

    for (name, guest_pc) in [("same-PC tail", PC), ("interior-PC tail", PC + 1)] {
        let mut function = canonical.clone();
        function.blocks[0]
            .ops
            .push(SmirOp::new(OpId(1), guest_pc, OpKind::Nop));
        malformed.push((name, function));
    }

    for (name, mutate) in [("width", 0_u8), ("next pc", 1), ("APX", 2)] {
        let mut function = canonical.clone();
        let OpKind::X86Leave(leave) = &mut function.blocks[0].ops[0].kind else {
            unreachable!()
        };
        match mutate {
            0 => leave.width = X86LeaveWidth::W64,
            1 => leave.next_pc += 1,
            2 => leave.requires_apx = true,
            _ => unreachable!(),
        }
        malformed.push((name, function));
    }

    let mut hinted = canonical.clone();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::ShiftGroup6);
    malformed.push(("hinted", hinted));

    for (name, function) in malformed {
        assert!(encoding(&function).is_none(), "{name}");
        assert!(!admitted(&function, true), "{name}");
        assert!(lower_with(&function, true, true).is_err(), "{name}");
    }
}

#[test]
fn leave_source_length_accepts_fifteen_bytes_and_rejects_sixteen() {
    let mut maximum = vec![0x66; 14];
    maximum.push(0xC9);
    let function = lift(&maximum);
    assert_eq!(encoding(&function).unwrap().next_pc, PC + 15);
    assert!(admitted(&function, true));

    let mut too_long = vec![0x66; 15];
    too_long.push(0xC9);
    let mut context = LiftContext::new(crate::smir::SourceArch::X86_64);
    assert!(
        X86_64Lifter::strict()
            .lift_insn(PC, &too_long, &mut context)
            .is_err()
    );
}
