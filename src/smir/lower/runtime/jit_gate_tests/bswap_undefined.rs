//! Native admission for the deterministic undefined-result profile of `BSWAP r16`.

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

const PC: u64 = 0xB500;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const LEGACY_PREFIXES: [&[u8]; 4] = [&[0x66], &[0x66, 0x41], &[0x48, 0x66], &[0xF3, 0x2E, 0x66]];

fn lift(bytes: &[u8]) -> SmirFunction {
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = X86_64Lifter::strict()
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("BSWAP r16 {bytes:02X?}: {error:?}"));
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
        X86InstructionBytes::new(bytes).expect("complete BSWAP r16 source"),
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
fn every_legacy_bswap_r16_register_admits_and_lowers_at_all_levels() {
    let mut checks = 0usize;
    for prefix in LEGACY_PREFIXES {
        for opcode in 0xC8_u8..=0xCF {
            let mut bytes = prefix.to_vec();
            bytes.extend_from_slice(&[0x0F, opcode]);
            let lifted = lift(&bytes);
            assert!(lifted.blocks[0].ops.is_empty(), "{bytes:02X?}");

            for level in LEVELS {
                let function = optimize(lifted.clone(), level);
                assert!(function.blocks[0].ops.is_empty(), "{bytes:02X?} {level:?}");
                assert_admitted_and_lowered(&function, &format!("{bytes:02X?} {level:?}"));
                checks += 1;
            }
        }
    }
    assert_eq!(checks, 4 * 8 * 3);
}

#[test]
fn every_rex2_bswap_r16_payload_admits_and_lowers_at_all_levels() {
    let mut checks = 0usize;
    for payload in 0x80_u8..=0xFF {
        if payload & 0x08 != 0 {
            continue;
        }
        for opcode in 0xC8_u8..=0xCF {
            let bytes = [0x66, 0xD5, payload, opcode];
            let lifted = lift(&bytes);
            assert!(matches!(
                lifted.blocks[0].ops.as_slice(),
                [op] if matches!(op.kind, OpKind::X86RequireApx)
            ));

            for level in LEVELS {
                let function = optimize(lifted.clone(), level);
                assert!(matches!(
                    function.blocks[0].ops.as_slice(),
                    [op] if matches!(op.kind, OpKind::X86RequireApx)
                ));
                assert_admitted_and_lowered(&function, &format!("REX2 {bytes:02X?} {level:?}"));
                checks += 1;
            }
        }
    }
    assert_eq!(checks, 64 * 8 * 3);
}
