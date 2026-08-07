//! Exact x86 ENTER admission, provenance, optimizer, and ABI coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::SmirOp;
use crate::smir::ir::types::OpId;
use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    GuestRegs, is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    x86_jit_op_uses_mem_helper,
};
use crate::smir::lower::x86_64::{X86EnterEncoding, x86_enter_encoding};
use crate::smir::lower::{
    SmirLowerer, X86_GUEST_ENTER_FN_OFFSET, X86_GUEST_IO_REQUEST_OFFSET, x86_64::X86_64Lowerer,
};
use crate::smir::optimize::{OptLevel, optimize_function};

const PC: u64 = 0x454E_5400;
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
        X86InstructionBytes::new(bytes).expect("complete ENTER source"),
    );
    function
}

fn encoding(function: &SmirFunction) -> Option<X86EnterEncoding> {
    x86_enter_encoding(&function.blocks[0], 0, &function.x86_instruction_bytes)
}

fn admitted(function: &SmirFunction, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(function, &HashMap::new(), allow_mem)
}

fn lower(function: &SmirFunction) -> Result<Vec<u8>, crate::smir::lower::LowerError> {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer.lower_function(function)?;
    lowerer.finalize()
}

#[test]
fn enter_abi_is_append_only_and_helper_classified() {
    assert_eq!(
        std::mem::offset_of!(GuestRegs, enter_fn),
        X86_GUEST_ENTER_FN_OFFSET as usize
    );
    assert_eq!(X86_GUEST_ENTER_FN_OFFSET, X86_GUEST_IO_REQUEST_OFFSET + 8);
    assert_eq!(GuestRegs::default().enter_fn, 0);
    let function = lift(&[0xC8, 0, 0, 0]);
    let op = &function.blocks[0].ops[0];
    assert!(x86_jit_op_uses_mem_helper(&op.kind));
    assert!(!op.kind.is_jit_safe());
    assert_eq!(
        op.kind.dests(),
        [
            VReg::Arch(ArchReg::X86(X86Reg::Rsp)),
            VReg::Arch(ArchReg::X86(X86Reg::Rbp)),
        ]
    );
    assert_eq!(op.kind.source_vregs(), op.kind.dests());
    assert!(op.kind.has_side_effects());
    assert!(op.kind.reads_memory());
    assert!(op.kind.writes_memory());
}

#[test]
fn all_3584_scanner_enter_images_are_exactly_admitted() {
    let mut images = 0usize;
    for prefix in SCANNER_PREFIXES {
        for raw_nesting in u8::MIN..=u8::MAX {
            let mut bytes = prefix.to_vec();
            bytes.extend([0xC8, 0x34, 0x12, raw_nesting]);
            let function = lift(&bytes);
            let decoded = encoding(&function)
                .unwrap_or_else(|| panic!("missing exact ENTER encoding: {bytes:02X?}"));
            assert_eq!(decoded.allocation_size, 0x1234);
            assert_eq!(decoded.nesting_level, raw_nesting & 0x1F);
            assert_eq!(decoded.next_pc, PC + bytes.len() as u64);
            assert!(!admitted(&function, false), "{bytes:02X?}");
            assert!(admitted(&function, true), "{bytes:02X?}");
            images += 1;
        }
    }
    assert_eq!(images, 14 * 256);
}

#[test]
fn enter_survives_all_optimizer_levels_and_lowers_exact_widths() {
    for bytes in [
        &[0xC8, 0x20, 0, 0][..],
        &[0x66, 0xC8, 0x10, 0, 2],
        &[0x66, 0x48, 0xC8, 0, 0, 31],
        &[0xD5, 0x00, 0xC8, 0, 0, 1],
    ] {
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut function = lift(bytes);
            optimize_function(&mut function, level);
            assert!(encoding(&function).is_some(), "{bytes:02X?} {level:?}");
            assert!(admitted(&function, true), "{bytes:02X?} {level:?}");
            let code = lower(&function).unwrap_or_else(|error| {
                panic!("{bytes:02X?} {level:?}: native lower failed: {error:?}")
            });
            let offset = X86_GUEST_ENTER_FN_OFFSET.to_le_bytes();
            assert!(
                code.windows(offset.len()).any(|window| window == offset),
                "{bytes:02X?} {level:?}: helper offset missing"
            );
        }
    }
}

#[test]
fn enter_rejects_missing_mismatched_and_cross_host_shapes() {
    let canonical = lift(&[0xC8, 0x20, 0, 2]);
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
        X86InstructionBytes::new(&[0xC8, 0x20, 0, 2, 0x90]).unwrap(),
    );
    malformed.push(("trailing source", trailing));

    for (name, guest_pc) in [("same-PC tail", PC), ("interior-PC tail", PC + 1)] {
        let mut function = canonical.clone();
        function.blocks[0]
            .ops
            .push(SmirOp::new(OpId(1), guest_pc, OpKind::Nop));
        malformed.push((name, function));
    }

    for (name, mutate) in [
        ("allocation", 0_u8),
        ("nesting", 1),
        ("width", 2),
        ("next pc", 3),
        ("APX", 4),
    ] {
        let mut function = canonical.clone();
        let OpKind::X86Enter(enter) = &mut function.blocks[0].ops[0].kind else {
            unreachable!()
        };
        match mutate {
            0 => enter.allocation_size ^= 1,
            1 => enter.nesting_level ^= 1,
            2 => enter.width = OpWidth::W32,
            3 => enter.next_pc += 1,
            4 => enter.requires_apx = true,
            _ => unreachable!(),
        }
        malformed.push((name, function));
    }

    for (name, function) in malformed {
        assert!(encoding(&function).is_none(), "{name}");
        assert!(!admitted(&function, true), "{name}");
        assert!(lower(&function).is_err(), "{name}");
    }
}
