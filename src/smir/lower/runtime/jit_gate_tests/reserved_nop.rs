//! Native admission and lowering coverage for the `0F 1D /r` Reserved NOP.

use super::*;
use crate::smir::SourceArch;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{BlockId, FunctionId};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::{OptLevel, optimize_function};

const PC: u64 = 0x1D00;
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
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = X86_64Lifter::strict()
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(
        matches!(result.control_flow, ControlFlow::Fallthrough),
        "{bytes:02X?}"
    );

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("complete Reserved NOP source"),
    );
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    optimize_function(&mut function, level);
    function
}

fn assert_admitted_and_lowered(function: &SmirFunction, label: &str) {
    assert!(is_native_clobber_safe(function), "{label}: admission");
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{label}: lower: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{label}: finalize: {error:?}"));
    assert!(!code.is_empty(), "{label}: empty host function");
}

#[test]
fn every_legacy_scanner_register_image_admits_and_lowers_at_all_levels() {
    let mut checks = 0usize;
    for prefix in SCANNER_PREFIXES {
        for modrm in 0xC0_u8..=0xFF {
            let mut bytes = prefix.to_vec();
            bytes.extend_from_slice(&[0x0F, 0x1D, modrm]);
            let lifted = lift(&bytes);
            assert!(lifted.blocks[0].ops.is_empty(), "{bytes:02X?}");

            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                let function = optimize(lifted.clone(), level);
                assert!(function.blocks[0].ops.is_empty(), "{bytes:02X?} {level:?}");
                assert_admitted_and_lowered(&function, &format!("{bytes:02X?} {level:?}"));
                checks += 1;
            }
        }
    }
    assert_eq!(checks, 14 * 64 * 3);
}

#[test]
fn rex2_reserved_nop_guard_admits_and_lowers_at_all_levels() {
    let lifted = lift(&[0xD5, 0xFF, 0x1D, 0xC0]);
    assert!(matches!(
        lifted.blocks[0].ops[0].kind,
        OpKind::X86RequireApx
    ));

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let function = optimize(lifted.clone(), level);
        assert!(matches!(
            function.blocks[0].ops[0].kind,
            OpKind::X86RequireApx
        ));
        assert_admitted_and_lowered(&function, &format!("REX2 {level:?}"));
    }
}
