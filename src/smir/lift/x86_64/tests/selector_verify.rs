//! Strict lift, metadata, optimizer, and interpreter coverage for VERR/VERW.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::{FlagSet, MaterializedFlags};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::ops::{X86SelectorVerifyKind, X86SelectorVerifyOp, X86SelectorVerifySource};
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_verify(result: &LiftResult) -> &X86SelectorVerifyOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86SelectorVerify(verify) => verify,
        other => panic!("expected one exact X86SelectorVerify op, got {other:?}"),
    }
}

fn verify_block(bytes: &[u8]) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict VERR/VERW lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn descriptor(type_: u8, dpl: u8, present: bool, system: bool) -> [u8; 8] {
    (u64::from(type_ & 0xF) << 40
        | u64::from(!system) << 44
        | u64::from(dpl & 3) << 45
        | u64::from(present) << 47)
        .to_le_bytes()
}

fn execute_register(
    bytes: &[u8],
    selector: u16,
    descriptor: Option<[u8; 8]>,
    configure: impl FnOnce(&mut SmirContext, &mut FlatMemory),
) -> (BlockResult, SmirContext) {
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.efer = 1 << 10;
    x86.cs_l = true;
    x86.cpl = 0;
    x86.gdtr_base = 0x2000;
    x86.gdtr_limit = 0xFF;
    context.write_vreg(x86_gpr(0), u64::from(selector));
    let mut memory = FlatMemory::with_base(0x2000, 0x200);
    if let Some(raw) = descriptor {
        memory.load(usize::from(selector >> 3) * 8, &raw);
    }
    configure(&mut context, &mut memory);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &verify_block(bytes));
    (result, context)
}

#[test]
fn selector_verify_strictly_lifts_register_prefix_and_apx_forms() {
    for (bytes, kind, src, requires_apx) in [
        (
            &[0x0F, 0x00, 0xE0][..],
            X86SelectorVerifyKind::Read,
            0,
            false,
        ),
        (
            &[0x66, 0x48, 0x0F, 0x00, 0xE8],
            X86SelectorVerifyKind::Write,
            0,
            false,
        ),
        (
            &[0x41, 0x0F, 0x00, 0xE7],
            X86SelectorVerifyKind::Read,
            15,
            false,
        ),
        (
            &[0x44, 0x0F, 0x00, 0xE8],
            X86SelectorVerifyKind::Write,
            0,
            false,
        ),
        (
            &[0xD5, 0x91, 0x00, 0xE7],
            X86SelectorVerifyKind::Read,
            31,
            true,
        ),
        (
            &[0xD5, 0x91, 0x00, 0xEF],
            X86SelectorVerifyKind::Write,
            31,
            true,
        ),
    ] {
        let result = lift_single(bytes).expect("valid selector verification must strictly lift");
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            exact_verify(&result),
            X86SelectorVerifyOp {
                kind: got_kind,
                source: X86SelectorVerifySource::Register { src: got_src },
                requires_apx: got_apx,
                next_pc,
            } if *got_kind == kind
                && *got_src == x86_gpr(src)
                && *got_apx == requires_apx
                && *next_pc == 0x1000 + bytes.len() as u64
        ));
    }

    for prefix in [0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x40, 0xF2, 0xF3] {
        let bytes = [prefix, 0x0F, 0x00, 0xE0];
        let result = lift_single(&bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "prefix {prefix:02X}");
        assert!(matches!(
            exact_verify(&result).source,
            X86SelectorVerifySource::Register { src } if src == x86_gpr(0)
        ));
    }
}

#[test]
fn selector_verify_lifts_fixed_two_byte_memory_and_segment_classification() {
    let direct = lift_single(&[0x0F, 0x00, 0x20]).unwrap();
    assert!(matches!(
        exact_verify(&direct),
        X86SelectorVerifyOp {
            kind: X86SelectorVerifyKind::Read,
            source: X86SelectorVerifySource::Memory {
                addr: Address::Direct(base),
                stack_segment: false,
            },
            ..
        } if *base == x86_gpr(0)
    ));

    let stack_default = lift_single(&[0x0F, 0x00, 0x6C, 0x8D, 0x7F]).unwrap();
    assert!(matches!(
        exact_verify(&stack_default),
        X86SelectorVerifyOp {
            kind: X86SelectorVerifyKind::Write,
            source: X86SelectorVerifySource::Memory {
                addr: Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    disp: 0x7F,
                    disp_size: DispSize::Disp8,
                },
                stack_segment: true,
            },
            ..
        } if *base == x86_gpr(5) && *index == x86_gpr(1)
    ));

    let ds_override = lift_single(&[0x3E, 0x0F, 0x00, 0x65, 0x00]).unwrap();
    assert!(matches!(
        exact_verify(&ds_override).source,
        X86SelectorVerifySource::Memory {
            stack_segment: false,
            ..
        }
    ));

    let addr32 = lift_single(&[0x67, 0x0F, 0x00, 0xA4, 0x8D, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert!(matches!(
        &exact_verify(&addr32).source,
        X86SelectorVerifySource::Memory {
            addr: Address::X86Addr32(inner),
            ..
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

    let apx = lift_single(&[0xD5, 0xB3, 0x00, 0x24, 0xD1]).unwrap();
    assert!(matches!(
        exact_verify(&apx),
        X86SelectorVerifyOp {
            source: X86SelectorVerifySource::Memory {
                addr: Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 8,
                    ..
                },
                ..
            },
            requires_apx: true,
            ..
        } if *base == x86_gpr(25) && *index == x86_gpr(26)
    ));
}

#[test]
fn selector_verify_rejects_lock_and_closes_reserved_group6_encodings() {
    for modrm in [0xE0, 0xE8] {
        assert!(matches!(
            lift_single(&[0xF0, 0x0F, 0x00, modrm]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
    for modrm in [0x30, 0x38, 0xF0, 0xF8] {
        let result = lift_single(&[0x0F, 0x00, modrm]).unwrap();
        assert_eq!(result.bytes_consumed, 3);
        assert!(result.ops.is_empty());
        assert!(matches!(
            result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }
    assert!(matches!(
        lift_single(&[0x0F, 0x00]),
        Err(LiftError::Incomplete {
            have: 2,
            need: 3,
            ..
        })
    ));
}

#[test]
fn selector_verify_metadata_is_zf_only_memory_observable_and_jit_safe() {
    let register = &lift_single(&[0x0F, 0x00, 0xE5]).unwrap().ops[0];
    assert_eq!(register.kind.source_vregs(), vec![x86_gpr(5)]);
    assert!(register.kind.dests().is_empty());
    assert_eq!(register.kind.flags_written(), FlagSet::ZF);
    assert!(register.kind.flags_read().is_empty());
    assert!(
        register.kind.reads_memory(),
        "the implicit descriptor read is observable"
    );
    assert!(!register.kind.writes_memory());
    assert!(register.kind.has_side_effects());
    assert!(register.is_jit_safe());

    let memory = &lift_single(&[0x0F, 0x00, 0x6C, 0x48, 0x08]).unwrap().ops[0];
    assert_eq!(memory.kind.source_vregs(), vec![x86_gpr(1), x86_gpr(0)]);
    assert_eq!(memory.kind.flags_written(), FlagSet::ZF);
    assert!(memory.kind.reads_memory());
    assert!(memory.kind.has_side_effects());
}

#[test]
fn selector_verify_interpreter_type_privilege_presence_and_flags_are_exact() {
    let initial = MaterializedFlags {
        cf: true,
        pf: false,
        af: true,
        zf: false,
        sf: true,
        of: true,
        df: true,
        ac: true,
    };
    for (name, opcode, raw, selector, cpl, expected_zf) in [
        (
            "read-only data",
            0xE0,
            descriptor(0x0, 3, true, false),
            0x13,
            3,
            true,
        ),
        (
            "write denied",
            0xE8,
            descriptor(0x0, 3, true, false),
            0x13,
            3,
            false,
        ),
        (
            "write data",
            0xE8,
            descriptor(0x2, 3, true, false),
            0x13,
            3,
            true,
        ),
        (
            "execute-only",
            0xE0,
            descriptor(0x8, 3, true, false),
            0x13,
            3,
            false,
        ),
        (
            "readable code",
            0xE0,
            descriptor(0xA, 3, true, false),
            0x13,
            3,
            true,
        ),
        (
            "code never writable",
            0xE8,
            descriptor(0xA, 3, true, false),
            0x13,
            3,
            false,
        ),
        (
            "system",
            0xE0,
            descriptor(0x2, 3, true, true),
            0x13,
            3,
            false,
        ),
        ("DPL", 0xE8, descriptor(0x2, 2, true, false), 0x13, 3, false),
        ("RPL", 0xE8, descriptor(0x2, 2, true, false), 0x13, 2, false),
        (
            "conforming",
            0xE0,
            descriptor(0xE, 0, true, false),
            0x13,
            3,
            true,
        ),
        (
            "not-present readable",
            0xE0,
            descriptor(0x0, 3, false, false),
            0x13,
            3,
            true,
        ),
        (
            "not-present writable",
            0xE8,
            descriptor(0x2, 3, false, false),
            0x13,
            3,
            true,
        ),
    ] {
        let (result, context) =
            execute_register(&[0x0F, 0x00, opcode], selector, Some(raw), |context, _| {
                let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                    unreachable!()
                };
                x86.cpl = cpl;
                context.flags.materialized = initial;
            });
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            "{name}"
        );
        assert_eq!(context.flags.materialized.zf, expected_zf, "{name}");
        let mut expected = initial;
        expected.zf = expected_zf;
        assert_eq!(
            context.flags.materialized.to_rflags(),
            expected.to_rflags(),
            "{name}"
        );
    }
}

#[test]
fn selector_verify_interpreter_selector_failures_do_not_access_descriptors() {
    for selector in [0_u16, 1, 2, 3, 0x100] {
        let (result, context) =
            execute_register(&[0x0F, 0x00, 0xE0], selector, None, |context, _| {
                let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                    unreachable!()
                };
                x86.gdtr_base = u64::MAX;
                x86.gdtr_limit = 0x0F;
                context.flags.materialized.zf = true;
            });
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert!(!context.flags.materialized.zf, "selector {selector:#x}");
    }

    let (result, context) = execute_register(&[0x0F, 0x00, 0xE0], 0x14, None, |context, _| {
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.ldtr_selector = 0;
        x86.ldtr_cache.unusable = true;
        x86.ldtr_cache.base = u64::MAX;
        x86.ldtr_cache.limit = u32::MAX;
        context.write_vreg(x86_gpr(0), 0x14);
        context.flags.materialized.zf = true;
    });
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert!(!context.flags.materialized.zf);
}

#[test]
fn selector_verify_interpreter_memory_mode_apx_and_fault_order_are_precise() {
    let block = verify_block(&[0x0F, 0x00, 0x20]);
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.efer = 1 << 10;
    x86.cs_l = true;
    x86.gdtr_base = 0x2000;
    x86.gdtr_limit = 0x1F;
    context.write_vreg(x86_gpr(0), 0x2040);
    let mut memory = FlatMemory::with_base(0x2000, 0x80);
    memory.load(0x10, &descriptor(0x2, 0, true, false));
    memory.load(0x40, &0x10_u16.to_le_bytes());
    let result = SmirInterpreter::new().execute_block(&mut context, &mut memory, &block);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert!(context.flags.materialized.zf);

    for (cr0, vm) in [(0, false), (1, true)] {
        let mut context = SmirContext::new_x86_64();
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = cr0;
        if vm {
            x86.rflags |= crate::isa::x86_64::flags::bits::VM;
        }
        context.write_vreg(x86_gpr(0), u64::MAX);
        context.flags.materialized.zf = true;
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut FlatMemory::new(1), &block);
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Undefined {
                addr: 0x1000,
                opcode: 0
            })
        ));
        assert!(
            context.flags.materialized.zf,
            "mode fault must not commit ZF"
        );
    }

    let apx = verify_block(&[0xD5, 0x91, 0x00, 0xE7]);
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.apx_enabled = false;
    context.flags.materialized.zf = true;
    let result = SmirInterpreter::new().execute_block(&mut context, &mut FlatMemory::new(1), &apx);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined { .. })
    ));
    assert!(context.flags.materialized.zf);
}

#[test]
fn selector_verify_interpreter_memory_faults_do_not_commit_zf() {
    for (name, block, gdt_base, rax, expected_addr) in [
        (
            "source read",
            verify_block(&[0x0F, 0x00, 0x20]),
            0x2000,
            0x3000,
            0x3000,
        ),
        (
            "descriptor read",
            verify_block(&[0x0F, 0x00, 0xE0]),
            0x3000,
            0x10,
            0x3010,
        ),
    ] {
        let mut context = SmirContext::new_x86_64();
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = 1;
        x86.efer = 1 << 10;
        x86.cs_l = true;
        x86.gdtr_base = gdt_base;
        x86.gdtr_limit = 0xFF;
        context.write_vreg(x86_gpr(0), rax);
        context.flags.materialized.zf = true;

        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::with_base(0x2000, 0x80),
            &block,
        );
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::MemoryFault {
                    addr,
                    write: false
                }) if addr == expected_addr
            ),
            "{name}: {result:?}"
        );
        assert!(context.flags.materialized.zf, "{name}");
    }
}

#[test]
fn selector_verify_interpreter_noncanonical_source_selects_ss_or_gp_without_commit() {
    for (bytes, expected_ss) in [
        (&[0x0F, 0x00, 0x65, 0x00][..], true),
        (&[0x3E, 0x0F, 0x00, 0x65, 0x00][..], false),
    ] {
        let mut context = SmirContext::new_x86_64();
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = 1;
        x86.efer = 1 << 10;
        x86.cs_l = true;
        context.write_vreg(x86_gpr(5), 0x0000_8000_0000_0000);
        context.flags.materialized.zf = true;
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            &verify_block(bytes),
        );
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::StackSegment { .. })) == expected_ss
        );
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::GeneralProtection { .. })
            ) == !expected_ss
        );
        assert!(context.flags.materialized.zf);
    }
}

#[test]
fn selector_verify_survives_o2_and_repeated_observable_descriptor_reads() {
    let verify = |pc: u64, kind| {
        OpKind::X86SelectorVerify(X86SelectorVerifyOp {
            kind,
            source: X86SelectorVerifySource::Register { src: x86_gpr(0) },
            requires_apx: false,
            next_pc: pc + 3,
        })
    };
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, verify(0x1000, X86SelectorVerifyKind::Read));
    builder.push_op(0x1003, verify(0x1003, X86SelectorVerifyKind::Write));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);
    assert_eq!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86SelectorVerify(..)))
            .count(),
        2,
        "faulting/observable implicit descriptor reads must not be eliminated"
    );
}
