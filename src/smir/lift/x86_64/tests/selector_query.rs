//! Strict lift, metadata, optimizer, and interpreter coverage for LAR/LSL.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::{FlagSet, MaterializedFlags};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::ops::{X86SelectorQueryKind, X86SelectorQueryOp, X86SelectorQuerySource};
use crate::smir::optimize::{OptLevel, optimize_function};

fn exact_query(result: &LiftResult) -> &X86SelectorQueryOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86SelectorQuery(query) => query,
        other => panic!("expected one exact X86SelectorQuery op, got {other:?}"),
    }
}

fn query_block(bytes: &[u8]) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict LAR/LSL lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn descriptor_raw(type_: u8, dpl: u8, present: bool, system: bool, limit: u32, flags: u8) -> u64 {
    u64::from(limit & 0xFFFF)
        | (u64::from(type_ & 0xF) << 40)
        | (u64::from(!system) << 44)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (u64::from((limit >> 16) & 0xF) << 48)
        | (u64::from(flags & 0xF) << 52)
}

fn access_rights(raw: u64) -> u64 {
    ((raw >> 40) & 0xFFFF) << 8
}

fn expanded_limit(raw: u64) -> u64 {
    let mut limit = (raw & 0xFFFF) | (((raw >> 48) & 0xF) << 16);
    if raw & (1 << 55) != 0 {
        limit = (limit << 12) | 0xFFF;
    }
    limit
}

fn execute_register(
    bytes: &[u8],
    selector: u16,
    descriptor: Option<u64>,
    high: Option<u64>,
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
        let offset = usize::from(selector >> 3) * 8;
        memory.load(offset, &raw.to_le_bytes());
        if let Some(high) = high {
            memory.load(offset + 8, &high.to_le_bytes());
        }
    }
    configure(&mut context, &mut memory);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &query_block(bytes));
    (result, context)
}

#[test]
fn selector_query_strictly_lifts_width_rex_and_apx_register_forms() {
    for (bytes, kind, dst, src, width, requires_apx) in [
        (
            &[0x0F, 0x02, 0xC8][..],
            X86SelectorQueryKind::AccessRights,
            1,
            0,
            OpWidth::W32,
            false,
        ),
        (
            &[0x66, 0x0F, 0x03, 0xD3],
            X86SelectorQueryKind::Limit,
            2,
            3,
            OpWidth::W16,
            false,
        ),
        (
            &[0x48, 0x0F, 0x02, 0xF7],
            X86SelectorQueryKind::AccessRights,
            6,
            7,
            OpWidth::W64,
            false,
        ),
        (
            &[0x45, 0x0F, 0x03, 0xF7],
            X86SelectorQueryKind::Limit,
            14,
            15,
            OpWidth::W32,
            false,
        ),
        (
            &[0xD5, 0xD5, 0x02, 0xF7],
            X86SelectorQueryKind::AccessRights,
            30,
            31,
            OpWidth::W32,
            true,
        ),
        (
            &[0xD5, 0xDD, 0x03, 0xF7],
            X86SelectorQueryKind::Limit,
            30,
            31,
            OpWidth::W64,
            true,
        ),
        (
            &[0x66, 0xD5, 0xD5, 0x02, 0xF7],
            X86SelectorQueryKind::AccessRights,
            30,
            31,
            OpWidth::W16,
            true,
        ),
    ] {
        let result = lift_single(bytes).expect("valid selector query must strictly lift");
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            exact_query(&result),
            X86SelectorQueryOp {
                kind: got_kind,
                dst: got_dst,
                source: X86SelectorQuerySource::Register { src: got_src },
                width: got_width,
                requires_apx: got_apx,
                next_pc,
            } if *got_kind == kind
                && *got_dst == x86_gpr(dst)
                && *got_src == x86_gpr(src)
                && *got_width == width
                && *got_apx == requires_apx
                && *next_pc == 0x1000 + bytes.len() as u64
        ));
    }

    for prefix in [0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x40, 0xF2, 0xF3] {
        let bytes = [prefix, 0x0F, 0x02, 0xC8];
        let result = lift_single(&bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "prefix {prefix:02X}");
        assert!(matches!(
            exact_query(&result),
            X86SelectorQueryOp {
                dst,
                source: X86SelectorQuerySource::Register { src },
                width: OpWidth::W32,
                ..
            } if *dst == x86_gpr(1) && *src == x86_gpr(0)
        ));
    }
}

#[test]
fn selector_query_lifts_fixed_two_byte_memory_address_and_segment_forms() {
    let direct = lift_single(&[0x0F, 0x02, 0x08]).unwrap();
    assert!(matches!(
        exact_query(&direct),
        X86SelectorQueryOp {
            kind: X86SelectorQueryKind::AccessRights,
            dst,
            source: X86SelectorQuerySource::Memory {
                addr: Address::Direct(base),
                stack_segment: false,
            },
            width: OpWidth::W32,
            ..
        } if *dst == x86_gpr(1) && *base == x86_gpr(0)
    ));

    let stack_default = lift_single(&[0x48, 0x0F, 0x03, 0x54, 0x8D, 0x7F]).unwrap();
    assert!(matches!(
        exact_query(&stack_default),
        X86SelectorQueryOp {
            kind: X86SelectorQueryKind::Limit,
            dst,
            source: X86SelectorQuerySource::Memory {
                addr: Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    disp: 0x7F,
                    disp_size: DispSize::Disp8,
                },
                stack_segment: true,
            },
            width: OpWidth::W64,
            ..
        } if *dst == x86_gpr(2) && *base == x86_gpr(5) && *index == x86_gpr(1)
    ));

    let ds_override = lift_single(&[0x3E, 0x0F, 0x03, 0x55, 0x00]).unwrap();
    assert!(matches!(
        exact_query(&ds_override).source,
        X86SelectorQuerySource::Memory {
            stack_segment: false,
            ..
        }
    ));

    let addr32 = lift_single(&[0x67, 0x0F, 0x02, 0x94, 0x8D, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert!(matches!(
        &exact_query(&addr32).source,
        X86SelectorQuerySource::Memory {
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

    let apx = lift_single(&[0xD5, 0xF7, 0x03, 0x9C, 0xD1, 0x20, 0, 0, 0]).unwrap();
    assert!(matches!(
        exact_query(&apx),
        X86SelectorQueryOp {
            kind: X86SelectorQueryKind::Limit,
            dst,
            source: X86SelectorQuerySource::Memory {
                addr: Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 8,
                    disp: 0x20,
                    ..
                },
                ..
            },
            requires_apx: true,
            ..
        } if *dst == x86_gpr(27) && *base == x86_gpr(25) && *index == x86_gpr(26)
    ));
}

#[test]
fn selector_query_rejects_lock_and_reports_incomplete_modrm() {
    for opcode in [0x02, 0x03] {
        assert!(matches!(
            lift_single(&[0xF0, 0x0F, opcode, 0xC0]),
            Err(LiftError::InvalidEncoding { .. })
        ));
        assert!(matches!(
            lift_single(&[0x0F, opcode]),
            Err(LiftError::Incomplete { .. })
        ));
        assert!(matches!(
            lift_single(&[0x48, 0xD5, 0x80, opcode, 0xC0]),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn selector_query_metadata_tracks_conditional_destination_zf_and_memory() {
    let register = &lift_single(&[0x0F, 0x02, 0xCD]).unwrap().ops[0];
    assert_eq!(register.kind.source_vregs(), vec![x86_gpr(1), x86_gpr(5)]);
    assert_eq!(register.kind.dests(), vec![x86_gpr(1)]);
    assert_eq!(register.kind.flags_written(), FlagSet::ZF);
    assert!(register.kind.flags_read().is_empty());
    assert!(register.kind.reads_memory());
    assert!(!register.kind.writes_memory());
    assert!(register.kind.has_side_effects());
    assert!(register.is_jit_safe());

    let memory = &lift_single(&[0x0F, 0x03, 0x54, 0x48, 0x08]).unwrap().ops[0];
    assert_eq!(
        memory.kind.source_vregs(),
        vec![x86_gpr(2), x86_gpr(1), x86_gpr(0)]
    );
    assert_eq!(memory.kind.dests(), vec![x86_gpr(2)]);
    assert_eq!(memory.kind.flags_written(), FlagSet::ZF);
    assert!(memory.kind.reads_memory());
    assert!(memory.kind.has_side_effects());
}

#[test]
fn selector_query_interpreter_width_values_presence_alias_and_flags_are_exact() {
    let lar_raw = descriptor_raw(0x2, 3, false, false, 0x54321, 0xD);
    let lsl_raw = descriptor_raw(0xA, 3, true, false, 0xABCDE, 0x8);
    let initial_flags = MaterializedFlags {
        cf: true,
        pf: false,
        af: true,
        zf: false,
        sf: true,
        of: true,
        df: true,
        ac: true,
    };
    let sentinel = 0xA5A5_5A5A_DEAD_BEEF;
    for (name, bytes, raw, expected) in [
        (
            "LAR W16",
            &[0x66, 0x0F, 0x02, 0xC8][..],
            lar_raw,
            (sentinel & !0xFFFF) | (access_rights(lar_raw) & 0xFFFF),
        ),
        (
            "LAR W32",
            &[0x0F, 0x02, 0xC8],
            lar_raw,
            access_rights(lar_raw),
        ),
        (
            "LAR W64",
            &[0x48, 0x0F, 0x02, 0xC8],
            lar_raw,
            access_rights(lar_raw),
        ),
        (
            "LSL W32 granular",
            &[0x0F, 0x03, 0xC8],
            lsl_raw,
            expanded_limit(lsl_raw),
        ),
    ] {
        let (result, context) = execute_register(bytes, 0x13, Some(raw), None, |context, _| {
            context.write_vreg(x86_gpr(0), 0xFFFF_FFFF_0000_0013);
            context.write_vreg(x86_gpr(1), sentinel);
            context.flags.materialized = initial_flags;
        });
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            "{name}"
        );
        assert_eq!(context.read_vreg(x86_gpr(1)), expected, "{name}");
        let mut expected_flags = initial_flags;
        expected_flags.zf = true;
        assert_eq!(
            context.flags.materialized.to_rflags(),
            expected_flags.to_rflags(),
            "{name}"
        );
    }

    let (result, context) = execute_register(
        &[0x0F, 0x03, 0xC0],
        0x13,
        Some(lsl_raw),
        None,
        |context, _| {
            context.write_vreg(x86_gpr(0), 0xCAFE_BABE_0000_0013);
        },
    );
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86_gpr(0)), expanded_limit(lsl_raw));
    assert!(context.flags.materialized.zf);
}

#[test]
fn selector_query_interpreter_invalid_selectors_preserve_destination_and_clear_only_zf() {
    let restricted = descriptor_raw(0x2, 2, true, false, 0x12345, 0);
    let invalid_type = descriptor_raw(0xE, 3, true, true, 0x12345, 0);
    let sentinel = 0xA5A5_5A5A_DEAD_BEEF;
    let initial = MaterializedFlags::from_rflags(0x4_0CD7);
    for (name, selector, raw, configure) in [
        ("null", 0_u16, None, 0_u8),
        ("out of bounds", 0x100, None, 1),
        ("invalid type", 0x13, Some(invalid_type), 0),
        ("DPL", 0x10, Some(restricted), 2),
        ("RPL", 0x13, Some(restricted), 3),
    ] {
        let (result, context) =
            execute_register(&[0x0F, 0x02, 0xC8], selector, raw, None, |context, _| {
                let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                    unreachable!()
                };
                match configure {
                    1 => x86.gdtr_limit = 0x0F,
                    2 => x86.cpl = 3,
                    3 => {
                        x86.cpl = 2;
                        context.write_vreg(x86_gpr(0), 0x13);
                    }
                    _ => {}
                }
                context.write_vreg(x86_gpr(1), sentinel);
                context.flags.materialized = initial;
            });
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            "{name}"
        );
        assert_eq!(context.read_vreg(x86_gpr(1)), sentinel, "{name}");
        let mut expected = initial;
        expected.zf = false;
        assert_eq!(
            context.flags.materialized.to_rflags(),
            expected.to_rflags(),
            "{name}"
        );
    }
}

#[test]
fn selector_query_interpreter_ia32e_system_descriptor_high_half_is_precise() {
    let ldt = descriptor_raw(0x2, 0, false, true, 0x34567, 0);
    let tss = descriptor_raw(0x9, 0, false, true, 0x23456, 0);
    for (bytes, raw, expected) in [
        (&[0x0F, 0x02, 0xC8][..], ldt, access_rights(ldt)),
        (&[0x0F, 0x03, 0xC8], tss, expanded_limit(tss)),
    ] {
        let (result, context) =
            execute_register(bytes, 0x10, Some(raw), Some(0x1234_5678), |_, _| {});
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        assert_eq!(context.read_vreg(x86_gpr(1)), expected);
        assert!(context.flags.materialized.zf);
    }

    for (name, high, limit) in [
        ("reserved upper field", Some(1_u64 << 40), 0xFF_u16),
        ("truncated descriptor", Some(0), 0x17),
    ] {
        let sentinel = 0xD00D_F00D_CAFE_BEEF;
        let (result, context) =
            execute_register(&[0x0F, 0x02, 0xC8], 0x10, Some(ldt), high, |context, _| {
                let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                    unreachable!()
                };
                x86.gdtr_limit = limit;
                context.write_vreg(x86_gpr(1), sentinel);
                context.flags.materialized.zf = true;
            });
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            "{name}"
        );
        assert_eq!(context.read_vreg(x86_gpr(1)), sentinel, "{name}");
        assert!(!context.flags.materialized.zf, "{name}");
    }

    let call_gate = descriptor_raw(0xC, 0, true, true, 0, 0);
    let (lar, lar_ctx) = execute_register(
        &[0x0F, 0x02, 0xC8],
        0x10,
        Some(call_gate),
        Some(0),
        |_, _| {},
    );
    assert!(matches!(lar, BlockResult::Exit(ExitReason::Halt)));
    assert!(lar_ctx.flags.materialized.zf);
    let (lsl, lsl_ctx) = execute_register(
        &[0x0F, 0x03, 0xC8],
        0x10,
        Some(call_gate),
        None,
        |context, _| context.write_vreg(x86_gpr(1), 0xBAD),
    );
    assert!(matches!(lsl, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(lsl_ctx.read_vreg(x86_gpr(1)), 0xBAD);
    assert!(!lsl_ctx.flags.materialized.zf);
}

#[test]
fn selector_query_interpreter_compatibility_uses_lma_not_cs_l_for_type_format() {
    let legacy_tss = descriptor_raw(0x1, 0, false, true, 0x34567, 0);
    let (legacy_result, legacy) = execute_register(
        &[0x0F, 0x03, 0xC8],
        0x10,
        Some(legacy_tss),
        None,
        |context, _| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.efer = 0;
            x86.cs_l = false;
            context.write_vreg(x86_gpr(1), 0xA5A5_5A5A_DEAD_BEEF);
        },
    );
    assert!(matches!(legacy_result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(legacy.read_vreg(x86_gpr(1)), expanded_limit(legacy_tss));
    assert!(legacy.flags.materialized.zf);

    let sentinel = 0xA5A5_5A5A_DEAD_BEEF;
    let (compat_result, compatibility) = execute_register(
        &[0x0F, 0x03, 0xC8],
        0x10,
        Some(legacy_tss),
        None,
        |context, _| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cs_l = false;
            context.write_vreg(x86_gpr(1), sentinel);
            context.flags.materialized.zf = true;
        },
    );
    assert!(matches!(compat_result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(compatibility.read_vreg(x86_gpr(1)), sentinel);
    assert!(!compatibility.flags.materialized.zf);
}

#[test]
fn selector_query_interpreter_mode_apx_and_memory_faults_do_not_commit() {
    let raw = descriptor_raw(0x2, 0, true, false, 0x12345, 0);
    for (name, block, configure, memory_base) in [
        (
            "real mode before source",
            query_block(&[0x0F, 0x02, 0x08]),
            0_u8,
            0_u64,
        ),
        (
            "virtual-8086 before source",
            query_block(&[0x0F, 0x02, 0x08]),
            1,
            0,
        ),
        (
            "APX before source",
            query_block(&[0xD5, 0xD5, 0x02, 0xF7]),
            2,
            0,
        ),
        ("source read", query_block(&[0x0F, 0x02, 0x08]), 3, 0x2000),
        (
            "descriptor read",
            query_block(&[0x0F, 0x02, 0xC8]),
            4,
            0x2000,
        ),
    ] {
        let mut context = SmirContext::new_x86_64();
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = if configure == 0 { 0 } else { 1 };
        x86.efer = 1 << 10;
        x86.cs_l = true;
        x86.gdtr_base = if configure == 4 { 0x3000 } else { 0x2000 };
        x86.gdtr_limit = 0xFF;
        if configure == 1 {
            x86.rflags |= crate::isa::x86_64::flags::bits::VM;
        }
        if configure == 2 {
            x86.apx_enabled = false;
        }
        context.write_vreg(x86_gpr(0), if configure == 3 { 0x3000 } else { 0x10 });
        context.write_vreg(x86_gpr(1), 0xA5A5_5A5A_DEAD_BEEF);
        context.write_vreg(x86_gpr(30), 0xA5A5_5A5A_DEAD_BEEF);
        context.write_vreg(x86_gpr(31), 0x10);
        context.flags.materialized.zf = true;
        let mut memory = FlatMemory::with_base(memory_base, 0x80);
        if configure == 3 {
            memory.load(0x10, &raw.to_le_bytes());
        }
        let result = SmirInterpreter::new().execute_block(&mut context, &mut memory, &block);
        let expected = match configure {
            0..=2 => matches!(
                result,
                BlockResult::Exit(ExitReason::Undefined {
                    addr: 0x1000,
                    opcode: 0
                })
            ),
            3 => matches!(
                result,
                BlockResult::Exit(ExitReason::MemoryFault {
                    addr: 0x3000,
                    write: false
                })
            ),
            4 => matches!(
                result,
                BlockResult::Exit(ExitReason::MemoryFault {
                    addr: 0x3010,
                    write: false
                })
            ),
            _ => unreachable!(),
        };
        assert!(expected, "{name}: {result:?}");
        assert_eq!(
            context.read_vreg(x86_gpr(1)),
            0xA5A5_5A5A_DEAD_BEEF,
            "{name}"
        );
        assert_eq!(
            context.read_vreg(x86_gpr(30)),
            0xA5A5_5A5A_DEAD_BEEF,
            "{name}"
        );
        assert!(context.flags.materialized.zf, "{name}");
    }

    let tss = descriptor_raw(0x9, 0, true, true, 0x12345, 0);
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.cr0 = 1;
    x86.efer = 1 << 10;
    x86.cs_l = true;
    x86.gdtr_base = 0x2000;
    x86.gdtr_limit = 0xFF;
    context.write_vreg(x86_gpr(0), 0x10);
    context.write_vreg(x86_gpr(1), 0xA5A5_5A5A_DEAD_BEEF);
    context.flags.materialized.zf = true;
    let mut memory = FlatMemory::with_base(0x2000, 0x18);
    memory.load(0x10, &tss.to_le_bytes());
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut memory,
        &query_block(&[0x0F, 0x03, 0xC8]),
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::MemoryFault {
            addr: 0x2018,
            write: false
        })
    ));
    assert_eq!(context.read_vreg(x86_gpr(1)), 0xA5A5_5A5A_DEAD_BEEF);
    assert!(context.flags.materialized.zf);
}

#[test]
fn selector_query_interpreter_noncanonical_source_selects_ss_or_gp() {
    for (bytes, expected_ss) in [
        (&[0x0F, 0x03, 0x5D, 0x00][..], true),
        (&[0x3E, 0x0F, 0x03, 0x5D, 0x00][..], false),
    ] {
        let mut context = SmirContext::new_x86_64();
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = 1;
        x86.efer = 1 << 10;
        x86.cs_l = true;
        context.write_vreg(x86_gpr(5), 0x0000_8000_0000_0000);
        context.write_vreg(x86_gpr(3), 0xA5A5_5A5A_DEAD_BEEF);
        context.flags.materialized.zf = true;
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(1),
            &query_block(bytes),
        );
        assert_eq!(
            matches!(result, BlockResult::Exit(ExitReason::StackSegment { .. })),
            expected_ss
        );
        assert_eq!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::GeneralProtection { .. })
            ),
            !expected_ss
        );
        assert_eq!(context.read_vreg(x86_gpr(3)), 0xA5A5_5A5A_DEAD_BEEF);
        assert!(context.flags.materialized.zf);
    }
}

#[test]
fn selector_query_survives_o2_and_preserves_conditional_destination_liveness() {
    let query = |pc: u64, kind| {
        OpKind::X86SelectorQuery(X86SelectorQueryOp {
            kind,
            dst: x86_gpr(1),
            source: X86SelectorQuerySource::Register { src: x86_gpr(0) },
            width: OpWidth::W32,
            requires_apx: false,
            next_pc: pc + 3,
        })
    };
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, query(0x1000, X86SelectorQueryKind::AccessRights));
    builder.push_op(0x1003, query(0x1003, X86SelectorQueryKind::Limit));
    builder.set_terminator(Terminator::Return {
        values: vec![x86_gpr(1)],
    });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);
    assert_eq!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86SelectorQuery(..)))
            .count(),
        2,
        "faulting descriptor reads and conditional destination writes must remain observable"
    );
}
