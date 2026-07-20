//! Strict lift, metadata, optimizer, and interpreter coverage for LMSW.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::ops::{X86LmswOp, X86LmswSource};
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_lmsw(result: &LiftResult) -> &X86LmswOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86Lmsw(lmsw) => lmsw,
        other => panic!("expected one exact X86Lmsw op, got {other:?}"),
    }
}

fn lmsw_block(bytes: &[u8]) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict LMSW lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn execute_register(
    bytes: &[u8],
    configure: impl FnOnce(&mut SmirContext),
) -> (BlockResult, SmirContext) {
    let mut context = SmirContext::new_x86_64();
    configure(&mut context);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &lmsw_block(bytes),
    );
    (result, context)
}

#[test]
fn lmsw_strictly_lifts_fixed_width_register_sources_and_rex_extensions() {
    for (bytes, src, requires_apx) in [
        (&[0x0F, 0x01, 0xF0][..], 0, false),
        (&[0x66, 0x0F, 0x01, 0xF5], 5, false),
        (&[0x4D, 0x0F, 0x01, 0xF7], 15, false),
        (&[0xD5, 0x91, 0x01, 0xF7], 31, true),
        (&[0xD5, 0x99, 0x01, 0xF0], 24, true),
    ] {
        let result = lift_single(bytes).expect("LMSW register form must strictly lift");
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            exact_lmsw(&result),
            X86LmswOp {
                source: X86LmswSource::Register { src: got_src },
                requires_apx: got_apx,
                next_pc,
            } if *got_src == x86_gpr(src)
                && *got_apx == requires_apx
                && *next_pc == 0x1000 + bytes.len() as u64
        ));
    }
}

#[test]
fn lmsw_strictly_lifts_state_backed_memory_sources_at_exact_addresses() {
    let direct = lift_single(&[0x0F, 0x01, 0x30]).unwrap();
    assert!(matches!(
        &exact_lmsw(&direct).source,
        X86LmswSource::Memory {
            addr: Address::Direct(base)
        } if *base == x86_gpr(0)
    ));

    let sib = lift_single(&[0x48, 0x0F, 0x01, 0x74, 0x88, 0x7F]).unwrap();
    assert_eq!(sib.bytes_consumed, 6);
    assert!(matches!(
        &exact_lmsw(&sib).source,
        X86LmswSource::Memory {
            addr: Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: 0x7F,
                disp_size: DispSize::Disp8,
            }
        } if *base == x86_gpr(0) && *index == x86_gpr(1)
    ));

    let addr32 = lift_single(&[0x67, 0x0F, 0x01, 0xB4, 0x8D, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert!(matches!(
        &exact_lmsw(&addr32).source,
        X86LmswSource::Memory {
            addr: Address::X86Addr32(inner)
        } if matches!(
            inner.as_ref(),
            Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: 0x1234_5678,
                disp_size: DispSize::Disp32,
            } if *base == x86_gpr(5) && *index == x86_gpr(1)
        )
    ));

    let apx = lift_single(&[0xD5, 0xB3, 0x01, 0x34, 0xD1]).unwrap();
    assert!(matches!(
        exact_lmsw(&apx),
        X86LmswOp {
            source: X86LmswSource::Memory {
                addr: Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 8,
                    disp: 0,
                    ..
                }
            },
            requires_apx: true,
            ..
        } if *base == x86_gpr(25) && *index == x86_gpr(26)
    ));
}

#[test]
fn lmsw_ignores_operand_size_rex_w_repeat_and_register_segment_prefixes() {
    for bytes in [
        &[0x66, 0x0F, 0x01, 0xF0][..],
        &[0x48, 0x0F, 0x01, 0xF0],
        &[0xF2, 0x0F, 0x01, 0xF0],
        &[0xF3, 0x0F, 0x01, 0xF0],
        &[0x64, 0x0F, 0x01, 0xF0],
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(matches!(
            exact_lmsw(&result).source,
            X86LmswSource::Register { src } if src == x86_gpr(0)
        ));
    }
    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x01, 0xF0]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}

#[test]
fn lmsw_metadata_tracks_exact_source_memory_and_serializing_side_effects() {
    let register = &lift_single(&[0x0F, 0x01, 0xF5]).unwrap().ops[0];
    assert_eq!(register.kind.source_vregs(), vec![x86_gpr(5)]);
    assert!(register.kind.dests().is_empty());
    assert!(register.kind.flags_read().is_empty());
    assert!(register.kind.flags_written().is_empty());
    assert!(register.kind.has_side_effects());
    assert!(!register.kind.reads_memory());
    assert!(!register.kind.writes_memory());
    assert!(register.is_jit_safe());

    let memory = &lift_single(&[0x0F, 0x01, 0x74, 0x48, 0x08]).unwrap().ops[0];
    assert_eq!(memory.kind.source_vregs(), vec![x86_gpr(1), x86_gpr(0)]);
    assert!(memory.kind.dests().is_empty());
    assert!(memory.kind.has_side_effects());
    assert!(memory.kind.reads_memory());
    assert!(!memory.kind.writes_memory());
    assert!(memory.is_jit_safe());
}

#[test]
fn lmsw_interpreter_updates_only_low_four_bits_cannot_clear_pe_and_preserves_flags() {
    let old_cr0 = 0xFEDC_BA98_7654_321F;
    let flags = MaterializedFlags {
        cf: true,
        zf: false,
        sf: true,
        of: true,
        pf: false,
        af: true,
        df: true,
        ac: true,
    };
    for (source, expected_low) in [(0_u64, 1_u64), (0xA, 0xB), (0xFFFF_FFFF_FFFF_FFFE, 0xF)] {
        let (result, context) = execute_register(&[0x0F, 0x01, 0xF3], |context| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cr0 = old_cr0;
            context.flags.materialized = flags;
            context.write_vreg(x86_gpr(3), source);
        });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.cr0, (old_cr0 & !0xF) | expected_low);
        assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
        assert!(context.flags.lazy.is_none());
    }
}

#[test]
fn lmsw_interpreter_orders_apx_then_cpl_before_source_and_allows_real_mode() {
    let bytes = [0xD5, 0x91, 0x01, 0xF7];
    let old_cr0 = 0x8000_0031;
    let run = |apx_enabled| {
        execute_register(&bytes, |context| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cr0 = old_cr0;
            x86.cpl = 3;
            x86.apx_enabled = apx_enabled;
            context.write_vreg(x86_gpr(31), 0xE);
        })
    };
    let (apx_fault, context) = run(false);
    assert!(matches!(
        apx_fault,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.cr0, old_cr0);

    let (cpl_fault, context) = run(true);
    assert!(matches!(
        cpl_fault,
        BlockResult::Exit(ExitReason::GeneralProtection {
            addr: 0x1000,
            error_code: 0
        })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.cr0, old_cr0);

    let (real_mode, context) = execute_register(&[0x0F, 0x01, 0xF3], |context| {
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = 0x8000_0030;
        x86.cpl = 3;
        context.write_vreg(x86_gpr(3), 0xB);
    });
    assert!(matches!(real_mode, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.cr0, 0x8000_003B);
}

#[test]
fn lmsw_interpreter_memory_source_reads_exactly_two_bytes_after_guards() {
    let block = lmsw_block(&[0x0F, 0x01, 0x30]);
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 0xFEDC_BA98_7654_3211;
    context.write_vreg(x86_gpr(0), 0x2000);
    let mut memory = FlatMemory::with_base(0x2000, 2);
    memory.load(0, &[0x0A, 0xFF]);

    let result = SmirInterpreter::new().execute_block(&mut context, &mut memory, &block);

    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.cr0, 0xFEDC_BA98_7654_321B);
    let mut observed = [0; 2];
    memory.read(0x2000, &mut observed).unwrap();
    assert_eq!(observed, [0x0A, 0xFF]);
}

#[test]
fn lmsw_interpreter_rejects_malformed_source_or_frontier_without_commit() {
    for malformed in [
        X86LmswOp {
            source: X86LmswSource::Register { src: VReg::virt(0) },
            requires_apx: false,
            next_pc: 0x1003,
        },
        X86LmswOp {
            source: X86LmswSource::Register { src: x86_gpr(16) },
            requires_apx: false,
            next_pc: 0x1003,
        },
        X86LmswOp {
            source: X86LmswSource::Register { src: x86_gpr(0) },
            requires_apx: false,
            next_pc: 0x1002,
        },
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, OpKind::X86Lmsw(malformed));
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let function = builder.finish();
        let mut context = SmirContext::new_x86_64();
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = 0x1231;
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            function.entry_block().unwrap(),
        );
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
        ));
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.cr0, 0x1231);
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86Lmsw(X86LmswOp {
            source: X86LmswSource::Register { src: x86_gpr(0) },
            requires_apx: false,
            next_pc: 0x1003,
        }),
    );
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut function = builder.finish();
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 0x1231;
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        function.entry_block().unwrap(),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.cr0, 0x1231);
}

#[test]
fn lmsw_survives_o2_with_register_or_memory_source() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86Lmsw(X86LmswOp {
            source: X86LmswSource::Register { src: x86_gpr(0) },
            requires_apx: false,
            next_pc: 0x1003,
        }),
    );
    builder.push_op(
        0x1003,
        OpKind::X86Lmsw(X86LmswOp {
            source: X86LmswSource::Memory {
                addr: Address::Direct(x86_gpr(3)),
            },
            requires_apx: false,
            next_pc: 0x1006,
        }),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    let unoptimized = function.clone();
    optimize_function(&mut function, OptLevel::O2);
    assert_eq!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86Lmsw(..)))
            .count(),
        2
    );

    let execute = |function: &crate::smir::ir::SmirFunction| {
        let mut context = SmirContext::new_x86_64();
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = 0xFEDC_BA98_7654_3211;
        context.write_vreg(x86_gpr(0), 0x2);
        context.write_vreg(x86_gpr(3), 0x2000);
        let mut memory = FlatMemory::with_base(0x2000, 2);
        memory.load(0, &[0x0C, 0xFF]);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            function.entry_block().unwrap(),
        );
        let ArchRegState::X86_64(x86) = &context.arch_regs else {
            unreachable!()
        };
        (
            format!("{result:?}"),
            x86.cr0,
            context.flags.materialized.to_rflags(),
        )
    };

    assert_eq!(execute(&function), execute(&unoptimized));
}
