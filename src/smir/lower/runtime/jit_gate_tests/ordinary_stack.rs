//! Exact ordinary PUSH admission, optimizer, lowering, and fail-closed coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, FunctionId, MemWidth, OpId, OpWidth, SrcOperand, VReg, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    is_native_clobber_safe_excluding, x86_jit_pop_sequence_len, x86_jit_push_memory_sequence_len,
    x86_jit_push_sequence_len,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{SmirLowerer, X86_GUEST_LOAD_FN_OFFSET, X86_GUEST_STORE_FN_OFFSET};
use crate::smir::optimize::{OptLevel, optimize_function};

const PC: u64 = 0x5055_5348;
const GROUP5_RSP_PREFIXES: &[&[u8]] = &[
    &[],
    &[0x66],
    &[0xF2],
    &[0xF3],
    &[0x67],
    &[0x64],
    &[0x65],
    &[0x48],
    &[0x44],
    &[0x66, 0x48],
    &[0xF2, 0x48],
    &[0xF3, 0x48],
];

fn rsp() -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Rsp))
}

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
        X86InstructionBytes::new(bytes).expect("complete ordinary stack source"),
    );
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    optimize_function(&mut function, level);
    function
}

fn virtual_counts(block: &SmirBlock) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for op in &block.ops {
        for register in op.kind.dests() {
            if matches!(register, VReg::Virtual(_)) {
                *definitions.entry(register).or_insert(0) += 1;
            }
        }
        for register in op.kind.source_vregs() {
            if matches!(register, VReg::Virtual(_)) {
                *uses.entry(register).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

fn push_sequence_len(function: &SmirFunction) -> Option<usize> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_push_sequence_len(block, 0, true, &definitions, &uses)
}

fn memory_push_sequence_len(function: &SmirFunction) -> Option<usize> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_push_memory_sequence_len(block, 0, true, &definitions, &uses)
}

fn pop_sequence_len(function: &SmirFunction) -> Option<usize> {
    let block = &function.blocks[0];
    let (definitions, uses) = virtual_counts(block);
    x86_jit_pop_sequence_len(block, 0, true, &definitions, &uses)
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

fn assert_helper_lowering(function: &SmirFunction, label: &str) {
    assert!(!admitted(function, false), "{label}: memory policy");
    assert!(admitted(function, true), "{label}: admission");
    assert!(lower(function, false, true).is_err(), "{label}: helpers");
    let unguarded = lower(function, true, false)
        .unwrap_or_else(|error| panic!("{label}: intrinsic helper guard: {error:?}"));
    let code = lower(function, true, true)
        .unwrap_or_else(|error| panic!("{label}: exact lowering: {error:?}"));
    assert_eq!(code, unguarded, "{label}: external guard independence");
    let helper_offset = X86_GUEST_STORE_FN_OFFSET.to_le_bytes();
    assert!(
        code.windows(helper_offset.len())
            .any(|window| window == helper_offset),
        "{label}: store-helper offset"
    );
}

fn assert_load_helper_lowering(function: &SmirFunction, label: &str) {
    assert!(!admitted(function, false), "{label}: memory policy");
    assert!(admitted(function, true), "{label}: admission");
    assert!(lower(function, false, true).is_err(), "{label}: helpers");
    let code = lower(function, true, true)
        .unwrap_or_else(|error| panic!("{label}: exact lowering: {error:?}"));
    let helper_offset = X86_GUEST_LOAD_FN_OFFSET.to_le_bytes();
    assert!(
        code.windows(helper_offset.len())
            .any(|window| window == helper_offset),
        "{label}: load-helper offset"
    );
}

#[test]
fn all_256_length_mismatch_images_optimize_admit_and_lower_exactly() {
    let mut images = 0usize;
    for low in 0_u8..=u8::MAX {
        let bytes = [0x66, 0x48, 0x68, low, 0x00, 0x00, 0x00];
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let function = optimize(lift(&bytes), level);
            assert_eq!(
                push_sequence_len(&function),
                Some(2),
                "{bytes:02X?} {level:?}"
            );
            assert_helper_lowering(&function, &format!("{bytes:02X?} {level:?}"));
            images += 1;
        }
    }
    assert_eq!(images, 768);
}

#[test]
fn every_group5_rsp_scanner_image_retains_snapshot_fusion_through_optimization() {
    let mut images = 0usize;
    for prefix in GROUP5_RSP_PREFIXES {
        let mut bytes = prefix.to_vec();
        bytes.extend_from_slice(&[0xFF, 0xF4]);
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let function = optimize(lift(&bytes), level);
            assert_eq!(
                push_sequence_len(&function),
                Some(3),
                "{bytes:02X?} {level:?}"
            );
            assert_helper_lowering(&function, &format!("{bytes:02X?} {level:?}"));
            images += 1;
        }
    }
    assert_eq!(images, 36);
}

#[test]
fn exact_group5_memory_push_widths_optimize_admit_and_lower() {
    for bytes in [
        &[0xFF, 0x34, 0x24][..],
        &[0x66, 0xFF, 0x34, 0x24][..],
        &[0x66, 0x48, 0xFF, 0x34, 0x24][..],
    ] {
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let function = optimize(lift(bytes), level);
            assert_eq!(
                memory_push_sequence_len(&function),
                Some(3),
                "{bytes:02X?} {level:?}"
            );
            assert_helper_lowering(&function, &format!("{bytes:02X?} {level:?}"));
        }
    }
}

#[test]
fn all_112_register_pop_rm_images_optimize_admit_and_lower() {
    const PREFIXES: &[&[u8]] = &[
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

    let mut images = 0usize;
    for prefix in PREFIXES {
        for rm in 0_u8..8 {
            let mut bytes = prefix.to_vec();
            bytes.extend_from_slice(&[0x8F, 0xC0 | rm]);
            let rex_b = matches!(*prefix, [0x41] | [0x4D]);
            let destination_is_rsp = rm == 4 && !rex_b;
            let word = *prefix == [0x66];
            let expected_len = if destination_is_rsp && word { 4 } else { 2 };

            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                let function = optimize(lift(&bytes), level);
                assert_eq!(
                    pop_sequence_len(&function),
                    Some(expected_len),
                    "{bytes:02X?} {level:?}"
                );
                assert_load_helper_lowering(&function, &format!("{bytes:02X?} {level:?}"));
                images += 1;
            }
        }
    }
    assert_eq!(images, 336);
}

#[test]
fn malformed_group5_rsp_snapshots_fail_closed() {
    let canonical = lift(&[0xFF, 0xF4]);
    let temporary = match canonical.blocks[0].ops[0].kind {
        OpKind::Mov {
            dst: temporary @ VReg::Virtual(_),
            ..
        } => temporary,
        ref other => panic!("canonical snapshot changed: {other:?}"),
    };
    let mut malformed = Vec::new();

    let mut wrong_width = canonical.clone();
    let OpKind::Mov { width, .. } = &mut wrong_width.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *width = OpWidth::W16;
    malformed.push(("snapshot width", wrong_width));

    let mut split_pc = canonical.clone();
    split_pc.blocks[0].ops[0].guest_pc += 1;
    malformed.push(("snapshot guest PC", split_pc));

    let mut store_rsp = canonical.clone();
    let OpKind::Store { src, .. } = &mut store_rsp.blocks[0].ops[2].kind else {
        unreachable!()
    };
    *src = rsp();
    malformed.push(("store bypasses snapshot", store_rsp));

    let mut extra_use = canonical.clone();
    extra_use.blocks[0].ops.push(SmirOp::new(
        OpId(3),
        PC,
        OpKind::Mov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            src: SrcOperand::Reg(temporary),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("snapshot has a second consumer", extra_use));

    let mut second_definition = canonical.clone();
    second_definition.blocks[0].ops.push(SmirOp::new(
        OpId(3),
        PC,
        OpKind::Mov {
            dst: temporary,
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("snapshot has a second definition", second_definition));

    for (name, function) in malformed {
        assert_eq!(push_sequence_len(&function), None, "{name}");
        assert!(!admitted(&function, true), "{name}: admission");
    }
}

#[test]
fn group5_rsp_snapshot_contract_is_width_and_flag_exact() {
    for (bytes, width, delta, mem_width) in [
        (&[0xFF, 0xF4][..], OpWidth::W64, 8, MemWidth::B8),
        (&[0x66, 0xFF, 0xF4][..], OpWidth::W16, 2, MemWidth::B2),
        (&[0x66, 0x48, 0xFF, 0xF4][..], OpWidth::W64, 8, MemWidth::B8),
    ] {
        let function = lift(bytes);
        let ops = &function.blocks[0].ops;
        let snapshot = match ops[0].kind {
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(source),
                width: got_width,
            } if source == rsp() && got_width == width => dst,
            ref other => panic!("{bytes:02X?}: {other:?}"),
        };
        assert!(matches!(
            ops[1].kind,
            OpKind::Sub {
                dst,
                src1,
                src2: SrcOperand::Imm(got_delta),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if dst == rsp() && src1 == rsp() && got_delta == delta
        ));
        assert!(matches!(
            ops[2].kind,
            OpKind::Store {
                src,
                addr: Address::Direct(base),
                width: got_width,
            } if src == snapshot && base == rsp() && got_width == mem_width
        ));
    }
}
