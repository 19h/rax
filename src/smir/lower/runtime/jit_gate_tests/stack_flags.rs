//! Exact PUSHF/POPF admission, provenance, lowering, optimizer, and ABI coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{SmirOp, X86StackFlagsKind};
use crate::smir::ir::types::OpId;
use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    GuestRegs, is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    x86_jit_op_uses_mem_helper,
};
use crate::smir::lower::x86_64::{X86_64Lowerer, X86StackFlagsEncoding, x86_stack_flags_encoding};
use crate::smir::lower::{
    SmirLowerer, X86_GUEST_ENTER_FN_OFFSET, X86_GUEST_STACK_FLAGS_FN_OFFSET,
    X86_GUEST_STACK_FLAGS_RFLAGS_OFFSET, X86_GUEST_STACK_FLAGS_RFLAGS_VALID_OFFSET,
};
use crate::smir::optimize::{OptLevel, optimize_function};

const PC: u64 = 0x5354_4B46;
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

fn lift(bytes: &[u8]) -> SmirFunction {
    let mut context = LiftContext::new(crate::smir::SourceArch::X86_64);
    let result = X86_64Lifter::strict()
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("complete stack-flags source"),
    );
    function
}

fn encoding(function: &SmirFunction) -> Option<X86StackFlagsEncoding> {
    x86_stack_flags_encoding(&function.blocks[0], 0, &function.x86_instruction_bytes)
}

fn admitted(function: &SmirFunction, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(function, &HashMap::new(), allow_mem)
}

fn lower(
    function: &SmirFunction,
    mem_helpers: bool,
    guards: bool,
) -> Result<Vec<u8>, crate::smir::lower::LowerError> {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_jit_fault_deopt_guards(guards);
    lowerer.lower_function(function)?;
    lowerer.finalize()
}

#[test]
fn stack_flags_abi_is_append_only_and_helper_classified() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, stack_flags_fn),
        X86_GUEST_STACK_FLAGS_FN_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, stack_flags_rflags),
        X86_GUEST_STACK_FLAGS_RFLAGS_OFFSET as usize
    );
    assert_eq!(
        std::mem::offset_of!(GuestRegs, stack_flags_rflags_valid),
        X86_GUEST_STACK_FLAGS_RFLAGS_VALID_OFFSET as usize
    );
    assert_eq!(
        X86_GUEST_STACK_FLAGS_FN_OFFSET,
        X86_GUEST_ENTER_FN_OFFSET + 8
    );
    assert_eq!(
        X86_GUEST_STACK_FLAGS_RFLAGS_OFFSET,
        X86_GUEST_STACK_FLAGS_FN_OFFSET + 8
    );
    assert_eq!(
        X86_GUEST_STACK_FLAGS_RFLAGS_VALID_OFFSET,
        X86_GUEST_STACK_FLAGS_RFLAGS_OFFSET + 8
    );
    let defaults = GuestRegs::default();
    assert_eq!(defaults.stack_flags_fn, 0);
    assert_eq!(defaults.stack_flags_rflags, 0);
    assert_eq!(defaults.stack_flags_rflags_valid, 0);

    for bytes in [&[0x9C][..], &[0x9D][..]] {
        let function = lift(bytes);
        let op = &function.blocks[0].ops[0];
        assert!(x86_jit_op_uses_mem_helper(&op.kind), "{bytes:02X?}");
        assert!(!op.kind.is_jit_safe(), "{bytes:02X?}");
    }
}

#[test]
fn all_scanner_images_are_exactly_admitted_and_lowered() {
    let mut images = 0usize;
    for prefix in SCANNER_PREFIXES {
        for opcode in [0x9C, 0x9D] {
            let mut bytes = prefix.to_vec();
            bytes.push(opcode);
            let function = lift(&bytes);
            let decoded = encoding(&function)
                .unwrap_or_else(|| panic!("missing exact encoding: {bytes:02X?}"));
            assert_eq!(decoded.next_pc, PC + bytes.len() as u64);
            assert!(!admitted(&function, false), "{bytes:02X?}");
            assert!(admitted(&function, true), "{bytes:02X?}");
            assert!(lower(&function, false, true).is_err(), "{bytes:02X?}");
            assert!(lower(&function, true, false).is_err(), "{bytes:02X?}");
            let code = lower(&function, true, true)
                .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
            let helper_offset = X86_GUEST_STACK_FLAGS_FN_OFFSET.to_le_bytes();
            assert!(
                code.windows(helper_offset.len())
                    .any(|window| window == helper_offset),
                "{bytes:02X?}: helper offset missing"
            );
            images += 1;
        }
    }
    assert_eq!(images, 28);
}

#[test]
fn complete_rex2_payload_space_is_exactly_admitted() {
    let mut images = 0usize;
    for payload in 0_u8..=0x7F {
        for opcode in [0x9C, 0x9D] {
            let bytes = [0x66, 0xD5, payload, opcode];
            let function = lift(&bytes);
            let decoded = encoding(&function)
                .unwrap_or_else(|| panic!("missing REX2 encoding: {bytes:02X?}"));
            assert!(decoded.requires_apx, "{bytes:02X?}");
            assert_eq!(
                decoded.width,
                if payload & 0x08 != 0 {
                    OpWidth::W64
                } else {
                    OpWidth::W16
                },
                "{bytes:02X?}"
            );
            assert!(admitted(&function, true), "{bytes:02X?}");
            images += 1;
        }
    }
    assert_eq!(images, 256);
}

#[test]
fn stack_flags_survives_all_optimizer_levels_and_cross_host_rejects() {
    for bytes in [
        &[0x9C][..],
        &[0x66, 0x9D][..],
        &[0x66, 0x48, 0x9D][..],
        &[0x66, 0xD5, 0x08, 0x9C][..],
    ] {
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut function = lift(bytes);
            optimize_function(&mut function, level);
            assert!(encoding(&function).is_some(), "{bytes:02X?} {level:?}");
            assert!(admitted(&function, true), "{bytes:02X?} {level:?}");
            assert!(
                !is_x86_aarch64_native_clobber_safe_excluding(&function, &HashMap::new()),
                "{bytes:02X?} {level:?}"
            );
            lower(&function, true, true)
                .unwrap_or_else(|error| panic!("{bytes:02X?} {level:?}: {error:?}"));
        }
    }
}

#[test]
fn stack_flags_rejects_missing_mismatched_and_overlapping_shapes() {
    let canonical = lift(&[0x66, 0x9D]);
    let mut malformed = Vec::new();

    let mut missing = canonical.clone();
    missing.x86_instruction_bytes.clear();
    malformed.push(("missing provenance", missing));

    let mut trailing_source = canonical.clone();
    trailing_source.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(&[0x66, 0x9D, 0x90]).unwrap(),
    );
    malformed.push(("trailing source", trailing_source));

    for (name, guest_pc) in [("same-PC tail", PC), ("interior-PC tail", PC + 1)] {
        let mut function = canonical.clone();
        function.blocks[0]
            .ops
            .push(SmirOp::new(OpId(1), guest_pc, OpKind::Nop));
        malformed.push((name, function));
    }

    for (name, mutate) in [("kind", 0_u8), ("width", 1), ("next PC", 2), ("APX", 3)] {
        let mut function = canonical.clone();
        let OpKind::X86StackFlags(stack) = &mut function.blocks[0].ops[0].kind else {
            unreachable!()
        };
        match mutate {
            0 => stack.kind = X86StackFlagsKind::Push,
            1 => stack.width = OpWidth::W64,
            2 => stack.next_pc += 1,
            3 => stack.requires_apx = true,
            _ => unreachable!(),
        }
        malformed.push((name, function));
    }

    let mut hinted = canonical.clone();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    malformed.push(("hint", hinted));

    for (name, function) in malformed {
        assert!(encoding(&function).is_none(), "{name}");
        assert!(!admitted(&function, true), "{name}");
        assert!(lower(&function, true, true).is_err(), "{name}");
    }
}
