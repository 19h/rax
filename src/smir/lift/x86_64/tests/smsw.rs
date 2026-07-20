//! Strict lift, metadata, optimizer, and interpreter coverage for SMSW.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::ops::{X86SmswOp, X86SmswTarget};
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_smsw(result: &LiftResult) -> &X86SmswOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86Smsw(smsw) => smsw,
        other => panic!("expected one exact X86Smsw op, got {other:?}"),
    }
}

fn smsw_block(bytes: &[u8]) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict SMSW lift");
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
        &smsw_block(bytes),
    );
    (result, context)
}

#[test]
fn smsw_strictly_lifts_register_widths_and_rex_extensions_exactly() {
    for (bytes, dst, width, requires_apx) in [
        (&[0x0F, 0x01, 0xE0][..], 0, OpWidth::W32, false),
        (&[0x66, 0x0F, 0x01, 0xE5], 5, OpWidth::W16, false),
        (&[0x4D, 0x0F, 0x01, 0xE7], 15, OpWidth::W64, false),
        (&[0xD5, 0x91, 0x01, 0xE7], 31, OpWidth::W32, true),
        (&[0xD5, 0x99, 0x01, 0xE0], 24, OpWidth::W64, true),
    ] {
        let result = lift_single(bytes).expect("SMSW register form must strictly lift");
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            &exact_smsw(&result),
            X86SmswOp {
                target: X86SmswTarget::Register {
                    dst: got_dst,
                    width: got_width,
                },
                requires_apx: got_apx,
            } if *got_dst == x86_gpr(dst)
                && *got_width == width
                && *got_apx == requires_apx
        ));
    }
}

#[test]
fn smsw_strictly_lifts_state_backed_memory_addresses_without_width_drift() {
    let direct = lift_single(&[0x0F, 0x01, 0x20]).unwrap();
    assert_eq!(direct.bytes_consumed, 3);
    assert!(matches!(
        &exact_smsw(&direct).target,
        X86SmswTarget::Memory {
            addr: Address::Direct(base)
        } if *base == x86_gpr(0)
    ));

    let sib = lift_single(&[0x48, 0x0F, 0x01, 0x64, 0x88, 0x7F]).unwrap();
    assert_eq!(sib.bytes_consumed, 6);
    assert!(matches!(
        &exact_smsw(&sib).target,
        X86SmswTarget::Memory {
            addr: Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: 0x7F,
                disp_size: DispSize::Disp8,
            }
        } if *base == x86_gpr(0) && *index == x86_gpr(1)
    ));

    let addr32 = lift_single(&[0x67, 0x0F, 0x01, 0xA4, 0x8D, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert_eq!(addr32.bytes_consumed, 9);
    assert!(matches!(
        &exact_smsw(&addr32).target,
        X86SmswTarget::Memory {
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

    let apx_memory = lift_single(&[0xD5, 0xB3, 0x01, 0x24, 0xD1]).unwrap();
    assert!(matches!(
        &exact_smsw(&apx_memory),
        X86SmswOp {
            target: X86SmswTarget::Memory {
                addr: Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 8,
                    disp: 0,
                    ..
                }
            },
            requires_apx: true,
        } if *base == x86_gpr(25) && *index == x86_gpr(26)
    ));
}

#[test]
fn smsw_honors_only_operand_size_rex_w_and_address_size_prefix_effects() {
    let rex_w_wins = lift_single(&[0x66, 0x48, 0x0F, 0x01, 0xE0]).unwrap();
    assert!(matches!(
        exact_smsw(&rex_w_wins).target,
        X86SmswTarget::Register {
            width: OpWidth::W64,
            ..
        }
    ));

    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // ignored on a register target
        0x40, 0xF2, 0xF3,
    ] {
        let bytes = [prefix, 0x0F, 0x01, 0xE0];
        assert_eq!(lift_single(&bytes).unwrap().bytes_consumed, bytes.len());
    }
    assert!(matches!(
        lift_single(&[0xF0, 0x0F, 0x01, 0xE0]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}

#[test]
fn smsw_metadata_exposes_direct_destination_and_faulting_memory_effects() {
    let register = &lift_single(&[0x66, 0x0F, 0x01, 0xE5]).unwrap().ops[0];
    assert!(register.kind.source_vregs().is_empty());
    assert_eq!(register.kind.dests(), vec![x86_gpr(5)]);
    assert!(register.kind.flags_read().is_empty());
    assert!(register.kind.flags_written().is_empty());
    assert!(register.kind.has_side_effects());
    assert!(!register.kind.reads_memory());
    assert!(!register.kind.writes_memory());
    assert!(register.is_jit_safe());

    let memory = &lift_single(&[0x0F, 0x01, 0x64, 0x48, 0x08]).unwrap().ops[0];
    assert_eq!(memory.kind.source_vregs(), vec![x86_gpr(1), x86_gpr(0)]);
    assert!(memory.kind.dests().is_empty());
    assert!(memory.kind.has_side_effects());
    assert!(!memory.kind.reads_memory());
    assert!(memory.kind.writes_memory());
    assert!(memory.is_jit_safe());
}

#[test]
fn smsw_interpreter_commits_exact_register_widths_and_preserves_flags() {
    let cr0 = 0xFEDC_BA98_7654_3211;
    let incoming = 0xA5A5_5A5A_DEAD_BEEF;
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
    for (bytes, dst, expected, apx) in [
        (
            &[0x66, 0x0F, 0x01, 0xE3][..],
            3,
            (incoming & !0xFFFF) | (cr0 & 0xFFFF),
            false,
        ),
        (&[0x0F, 0x01, 0xE3], 3, cr0 & u32::MAX as u64, false),
        (&[0x48, 0x0F, 0x01, 0xE3], 3, cr0, false),
        (&[0xD5, 0x91, 0x01, 0xE7], 31, cr0 & u32::MAX as u64, true),
    ] {
        let (result, context) = execute_register(bytes, |context| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cr0 = cr0;
            x86.apx_enabled = apx;
            context.flags.materialized = flags;
            context.write_vreg(x86_gpr(dst), incoming);
        });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(context.read_vreg(x86_gpr(dst)), expected, "{bytes:02X?}");
        assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
        assert!(context.flags.lazy.is_none());
    }
}

#[test]
fn smsw_interpreter_apx_and_umip_guards_are_ordered_and_noncommitting() {
    let bytes = [0xD5, 0x91, 0x01, 0xE7];
    let sentinel = 0x3131_3131_3131_3131;
    let run = |apx_enabled| {
        execute_register(&bytes, |context| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cr0 = 1;
            x86.cr4 = 1 << 11;
            x86.cpl = 3;
            x86.apx_enabled = apx_enabled;
            context.write_vreg(x86_gpr(31), sentinel);
        })
    };
    let (apx_fault, context) = run(false);
    assert!(matches!(
        apx_fault,
        BlockResult::Exit(ExitReason::Undefined {
            addr: 0x1000,
            opcode: 0
        })
    ));
    assert_eq!(context.read_vreg(x86_gpr(31)), sentinel);

    let (umip_fault, context) = run(true);
    assert!(matches!(
        umip_fault,
        BlockResult::Exit(ExitReason::GeneralProtection {
            addr: 0x1000,
            error_code: 0
        })
    ));
    assert_eq!(context.read_vreg(x86_gpr(31)), sentinel);

    let (real_mode, context) = execute_register(&[0x0F, 0x01, 0xE3], |context| {
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = 0x8000_0030;
        x86.cr4 = 1 << 11;
        x86.cpl = 3;
        context.write_vreg(x86_gpr(3), sentinel);
    });
    assert!(matches!(real_mode, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86_gpr(3)), 0x8000_0030);
}

#[test]
fn smsw_interpreter_memory_form_writes_exactly_two_bytes_after_guards() {
    let block = smsw_block(&[0x0F, 0x01, 0x20]);
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 0xFEDC_BA98_7654_3211;
    context.write_vreg(x86_gpr(0), 0x2001);
    let mut memory = FlatMemory::with_base(0x2000, 4);
    memory.load(0, &[0xA5; 4]);
    let result = SmirInterpreter::new().execute_block(&mut context, &mut memory, &block);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let mut observed = [0; 4];
    memory.read(0x2000, &mut observed).unwrap();
    assert_eq!(observed, [0xA5, 0x11, 0x32, 0xA5]);

    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.cr4 = 1 << 11;
    x86.cpl = 3;
    memory.load(0, &[0x5A; 4]);
    let fault = SmirInterpreter::new().execute_block(&mut context, &mut memory, &block);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
    ));
    memory.read(0x2000, &mut observed).unwrap();
    assert_eq!(observed, [0x5A; 4]);
}

#[test]
fn smsw_interpreter_rejects_malformed_architectural_targets_before_commit() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86Smsw(X86SmswOp {
            target: X86SmswTarget::Register {
                dst: x86_gpr(16),
                width: OpWidth::W64,
            },
            requires_apx: false,
        }),
    );
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let function = builder.finish();
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 0x1234;
    context.write_vreg(x86_gpr(16), 0xA5A5);

    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        function.entry_block().unwrap(),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined {
            addr: 0x1000,
            opcode: 0
        })
    ));
    assert_eq!(context.read_vreg(x86_gpr(16)), 0xA5A5);
}

#[test]
fn smsw_survives_o2_with_dead_register_or_memory_outputs() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86Smsw(X86SmswOp {
            target: X86SmswTarget::Register {
                dst: x86_gpr(0),
                width: OpWidth::W32,
            },
            requires_apx: false,
        }),
    );
    builder.push_op(
        0x1003,
        OpKind::X86Smsw(X86SmswOp {
            target: X86SmswTarget::Memory {
                addr: Address::Direct(x86_gpr(3)),
            },
            requires_apx: false,
        }),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);
    assert_eq!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86Smsw(..)))
            .count(),
        2,
        "dynamic faults and the memory effect prohibit dead-op elimination"
    );
}
