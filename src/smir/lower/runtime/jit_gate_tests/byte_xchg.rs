//! Exhaustive native admission for low-byte register `XCHG r/m8,r8`.

use super::*;
use crate::smir::SourceArch;
use crate::smir::ir::ops::{OpKind, X86OpHint};
use crate::smir::ir::types::{ArchReg, BlockId, FunctionId, OpWidth, VReg, VirtualId, X86Reg};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::{OptLevel, optimize_function};

const PC: u64 = 0xB860;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const LEGACY_PREFIXES: [&[u8]; 7] = [&[], &[0x66], &[0xF2], &[0xF3], &[0x67], &[0x64], &[0x65]];
const SCANNER_REX_PREFIXES: [&[u8]; 7] = [
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
        .unwrap_or_else(|error| panic!("byte XCHG {bytes:02X?}: {error:?}"));
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
        X86InstructionBytes::new(bytes).expect("complete byte XCHG source"),
    );
    function
}

fn assert_admitted_and_lowered(mut function: SmirFunction, level: OptLevel, label: &str) {
    optimize_function(&mut function, level);
    assert!(
        is_native_clobber_safe(&function),
        "{label} {level:?}: admission"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{label} {level:?}: lower: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{label} {level:?}: finalize: {error:?}"));
    assert!(!code.is_empty(), "{label} {level:?}: empty host function");
}

fn assert_exact_xchg_shape(function: &SmirFunction, requires_apx: bool, bytes: &[u8]) {
    let expected_len = usize::from(requires_apx) + 1;
    assert_eq!(
        function.blocks[0].ops.len(),
        expected_len,
        "{bytes:02X?}: {:?}",
        function.blocks[0].ops
    );
    if requires_apx {
        assert!(matches!(
            function.blocks[0].ops[0].kind,
            OpKind::X86RequireApx
        ));
    }
    assert!(matches!(
        function.blocks[0].ops[expected_len - 1].kind,
        OpKind::Xchg {
            width: OpWidth::W8,
            ..
        }
    ));
}

#[test]
fn all_560_legacy_scanner_gaps_admit_and_lower_at_every_optimization_level() {
    let mut encodings = 0usize;

    // Without REX, only ModR/M codes 0-3 name low bytes. Codes 4-7 are the
    // separate AH/CH/DH/BH replay family and are intentionally not counted.
    for prefix in LEGACY_PREFIXES {
        for reg in 0u8..4 {
            for rm in 0u8..4 {
                let mut bytes = prefix.to_vec();
                bytes.extend_from_slice(&[0x86, 0xC0 | (reg << 3) | rm]);
                let function = lift(&bytes);
                assert_exact_xchg_shape(&function, false, &bytes);
                for level in LEVELS {
                    assert_admitted_and_lowered(function.clone(), level, &format!("{bytes:02X?}"));
                }
                encodings += 1;
            }
        }
    }

    // These are the exact REX-bearing prefix images in the residual census.
    // Any REX prefix selects the low-byte namespace for all 64 ModR/M cells.
    for prefix in SCANNER_REX_PREFIXES {
        for modrm in 0xC0_u8..=0xFF {
            let mut bytes = prefix.to_vec();
            bytes.extend_from_slice(&[0x86, modrm]);
            let function = lift(&bytes);
            assert_exact_xchg_shape(&function, false, &bytes);
            for level in LEVELS {
                assert_admitted_and_lowered(function.clone(), level, &format!("{bytes:02X?}"));
            }
            encodings += 1;
        }
    }

    assert_eq!(encodings, 7 * 16 + 7 * 64);
    assert_eq!(encodings, 560);
}

#[test]
fn all_8192_rex2_payload_modrm_cells_admit_and_lower_at_every_optimization_level() {
    let mut encodings = 0usize;
    for payload in 0x00_u8..=0x7F {
        for modrm in 0xC0_u8..=0xFF {
            let bytes = [0xD5, payload, 0x86, modrm];
            let function = lift(&bytes);
            assert_exact_xchg_shape(&function, true, &bytes);
            for level in LEVELS {
                assert_admitted_and_lowered(function.clone(), level, &format!("{bytes:02X?}"));
            }
            encodings += 1;
        }
    }
    assert_eq!(encodings, 128 * 64);
}

#[test]
fn malformed_byte_xchg_ir_remains_fail_closed() {
    let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
    for kind in [
        OpKind::Xchg {
            reg1: x86(X86Reg::Rax),
            reg2: VReg::Virtual(VirtualId(0)),
            width: OpWidth::W8,
        },
        OpKind::Xchg {
            reg1: x86(X86Reg::Rax),
            reg2: x86(X86Reg::Rcx),
            width: OpWidth::W128,
        },
    ] {
        let mut block = SmirBlock::new(BlockId(0), PC);
        block.ops.push(crate::smir::ir::ops::SmirOp::new(
            crate::smir::ir::types::OpId(0),
            PC,
            kind,
        ));
        block.set_terminator(Terminator::Return { values: Vec::new() });
        let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
        function.add_block(block);
        assert!(!is_native_clobber_safe(&function));
    }

    let mut hinted = lift(&[0xD5, 0x10, 0x86, 0xC0]);
    hinted.blocks[0].ops[1].x86_hint = Some(X86OpHint::Mulx);
    assert!(!is_native_clobber_safe(&hinted));
}
