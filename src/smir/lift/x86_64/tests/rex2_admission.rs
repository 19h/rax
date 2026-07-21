//! Architectural REX2 applicability and dynamic-admission coverage.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::optimize::{OptLevel, optimize_function};

fn result_retains_apx_requirement(result: &LiftResult) -> bool {
    let first_op = result.ops.first().is_some_and(|op| match &op.kind {
        OpKind::X86RequireApx => true,
        OpKind::X86Cli { requires_apx, .. }
        | OpKind::X86Sti { requires_apx, .. }
        | OpKind::X86FsGsBase { requires_apx, .. } => *requires_apx,
        OpKind::X86Smsw(op) => op.requires_apx,
        OpKind::X86SystemSelectorStore(op) => op.requires_apx,
        OpKind::X86SystemSelectorLoad(op) => op.requires_apx,
        OpKind::X86SelectorVerify(op) => op.requires_apx,
        OpKind::X86SelectorQuery(op) => op.requires_apx,
        OpKind::X86FarJump(op) => op.requires_apx,
        OpKind::X86FarCall(op) => op.requires_apx,
        OpKind::X86FarReturn(op) => op.requires_apx,
        OpKind::X86Lmsw(op) => op.requires_apx,
        OpKind::X86DescriptorTableStore(op) => op.requires_apx,
        OpKind::X86DescriptorTableLoad(op) => op.requires_apx,
        _ => false,
    });
    first_op
        || matches!(
            &result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            } | ControlFlow::Trap {
                kind: TrapKind::X86Debug {
                    requires_apx: true,
                    ..
                } | TrapKind::X86SoftwareInterrupt {
                    requires_apx: true,
                    ..
                } | TrapKind::X86InterruptReturn {
                    requires_apx: true,
                    ..
                } | TrapKind::X86StringIo {
                    requires_apx: true,
                    ..
                }
            }
        )
}

#[test]
fn reserved_rex2_rows_prefix_bytes_and_xsave_groups_trap_before_operand_decode() {
    let map0_prefix = |opcode| {
        matches!(
            opcode,
            0x0F | 0x26 | 0x2E | 0x36 | 0x3E | 0x62 | 0x64
                ..=0x67 | 0xC4 | 0xC5 | 0xD5 | 0xF0 | 0xF2 | 0xF3
        )
    };
    for opcode in 0_u8..=u8::MAX {
        let reserved = matches!(opcode & 0xF0, 0x40 | 0x70 | 0xE0)
            || opcode & 0xF0 == 0xA0 && opcode != 0xA1
            || map0_prefix(opcode);
        if reserved {
            let result = lift_single(&[0xD5, 0x00, opcode, 0xFF, 0xFF])
                .unwrap_or_else(|error| panic!("map 0 opcode {opcode:#04x}: {error:?}"));
            assert_invalid_opcode_trap(&result, 3);
        }

        if matches!(opcode & 0xF0, 0x30 | 0x80) {
            let result = lift_single(&[0xD5, 0x80, opcode, 0xFF, 0xFF])
                .unwrap_or_else(|error| panic!("map 1 opcode {opcode:#04x}: {error:?}"));
            assert_invalid_opcode_trap(&result, 3);
        }
    }

    for (opcode, groups) in [(0xAE, &[4_u8, 5, 6][..]), (0xC7, &[3_u8, 4, 5][..])] {
        for mod_bits in 0_u8..=2 {
            for &group in groups {
                let modrm = mod_bits << 6 | group << 3;
                let result =
                    lift_single(&[0xD5, 0x80, opcode, modrm, 0xFF, 0xFF]).unwrap_or_else(|error| {
                        panic!("XSAVE-family opcode={opcode:#04x}, ModR/M={modrm:#04x}: {error:?}")
                    });
                assert_invalid_opcode_trap(&result, 4);
            }
        }
    }

    let prefixed_escape = lift_single(&[0x66, 0xD5, 0x00, 0x0F, 0xFF])
        .expect("legacy prefix before REX2 remains legal; 0F after REX2 is #UD");
    assert_invalid_opcode_trap(&prefixed_escape, 4);

    let jmpabs = lift_single(&[
        0xD5, 0x00, 0xA1, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11,
    ])
    .expect("map 0 A1 is the reserved-row JMPABS exception");
    assert_rex2_guarded_ops(&jmpabs, 0);
    assert!(matches!(
        jmpabs.control_flow,
        ControlFlow::Branch {
            target: 0x1122_3344_5566_7788
        }
    ));

    for bytes in [&[0xD5, 0x80, 0xAE, 0xE8][..], &[0xD5, 0x80, 0xC7, 0xF0]] {
        let result = lift_single(bytes)
            .unwrap_or_else(|error| panic!("register-form XSAVE neighbor {bytes:02X?}: {error:?}"));
        assert!(result_retains_apx_requirement(&result), "{bytes:02X?}");
        assert!(!matches!(
            result.control_flow,
            ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode
            }
        ));
    }
}

#[test]
fn every_successful_rex2_opcode_root_retains_exactly_ordered_dynamic_admission() {
    let mut successes = [0_usize; 2];
    for (map_index, payload) in [0x00_u8, 0x80].into_iter().enumerate() {
        for opcode in 0_u8..=u8::MAX {
            let mut bytes = vec![0xD5, payload, opcode];
            bytes.resize(15, 0);
            let Ok(result) = lift_single(&bytes) else {
                continue;
            };
            successes[map_index] += 1;
            assert!(
                result_retains_apx_requirement(&result),
                "successful REX2 map {map_index} opcode {opcode:#04x} lost APX admission: {result:?}"
            );

            let guards = result
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::X86RequireApx))
                .count();
            assert!(guards <= 1, "map {map_index} opcode {opcode:#04x}");
            if guards == 1 {
                assert!(matches!(
                    result.ops.first(),
                    Some(SmirOp {
                        id: OpId(0),
                        guest_pc: 0x1000,
                        kind: OpKind::X86RequireApx,
                        x86_hint: None,
                    })
                ));
                for (index, op) in result.ops.iter().enumerate() {
                    assert_eq!(
                        op.id,
                        OpId(index as u16),
                        "map {map_index} opcode {opcode:#04x}"
                    );
                }
            }
        }
    }
    assert!(successes.into_iter().all(|count| count != 0));
}

fn guarded_memory_function() -> SmirFunction {
    // MOV r64,[r16]. The noncanonical R16 value used below would fault if the
    // dynamic APX guard did not execute before address evaluation and memory.
    let result = lift_single(&[0xD5, 0x18, 0x8B, 0x00]).expect("REX2 MOV from [R16]");
    assert_rex2_guarded_ops(&result, 1);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for op in result.ops {
        builder.push_op(op.guest_pc, op.kind);
    }
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    builder.finish()
}

#[test]
fn generic_rex2_guard_precedes_memory_and_survives_o2_without_commit() {
    let original = guarded_memory_function();
    let mut optimized = original.clone();
    optimize_function(&mut optimized, OptLevel::O2);

    for function in [&original, &optimized] {
        assert!(matches!(
            function.entry_block().unwrap().ops.first(),
            Some(SmirOp {
                kind: OpKind::X86RequireApx,
                ..
            })
        ));
        for apx_enabled in [false, true] {
            let mut context = SmirContext::new_x86_64();
            context.write_vreg(x86_gpr(0), 0xA5A5_5A5A_DEAD_BEEF);
            context.write_vreg(x86_gpr(16), u64::MAX);
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.apx_enabled = apx_enabled;
            context.flags.materialized = MaterializedFlags::from_rflags(0x0CD7);

            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut FlatMemory::new(1),
                function.entry_block().unwrap(),
            );
            if apx_enabled {
                assert!(matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
                ));
            } else {
                assert!(matches!(
                    result,
                    BlockResult::Exit(ExitReason::Undefined {
                        addr: 0x1000,
                        opcode: 0
                    })
                ));
            }
            assert_eq!(context.read_vreg(x86_gpr(0)), 0xA5A5_5A5A_DEAD_BEEF);
            assert_eq!(context.read_vreg(x86_gpr(16)), u64::MAX);
            assert_eq!(context.flags.materialized.to_rflags(), 0x0CD7);
            assert!(context.flags.lazy.is_none());
        }
    }
}
