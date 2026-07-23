//! Strict lifting, interpretation, optimization, and native coverage for APX counts.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::x86_64::X86_64Lowerer;

fn count_encoding(width: OpWidth, opcode: u8, nf: bool, memory: bool) -> Vec<u8> {
    let p1 = match width {
        OpWidth::W16 => 0x7D,
        OpWidth::W32 => 0x7C,
        OpWidth::W64 => 0xFC,
        _ => unreachable!(),
    };
    let p2 = 0x08 | if nf { 0x04 } else { 0 };
    let modrm = if memory { 0x03 } else { 0xC3 };
    vec![0x62, 0x74, p1, p2, opcode, modrm]
}

fn expected_flags(kind: X86CountKind, nf: bool) -> FlagUpdate {
    if nf {
        FlagUpdate::None
    } else if kind == X86CountKind::Popcnt {
        FlagUpdate::All
    } else {
        FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF))
    }
}

fn assert_guarded_count(
    result: &LiftResult,
    instruction_len: usize,
    width: OpWidth,
    kind: X86CountKind,
    nf: bool,
    memory: bool,
) {
    assert_eq!(result.bytes_consumed, instruction_len);
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert!(matches!(
        result.ops.first(),
        Some(SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::X86RequireApx,
            x86_hint: None,
        })
    ));
    let count = result.ops.last().expect("terminal count operation");
    assert!(matches!(
        count.kind,
        OpKind::X86Count {
            dst,
            width: got_width,
            kind: got_kind,
            flags,
            ..
        } if dst == x86_gpr(8)
            && got_width == width
            && got_kind == kind
            && flags == expected_flags(kind, nf)
    ));
    if memory {
        assert!(matches!(
            result.ops.get(1),
            Some(SmirOp {
                kind: OpKind::Load {
                    addr: Address::Direct(base),
                    width: got_width,
                    sign: SignExtend::Zero,
                    ..
                },
                ..
            }) if *base == x86_gpr(3) && *got_width == width.to_mem_width()
        ));
        let loaded = match result.ops[1].kind {
            OpKind::Load { dst, .. } => dst,
            _ => unreachable!(),
        };
        assert!(matches!(count.kind, OpKind::X86Count { src, .. } if src == loaded));
    } else {
        assert!(matches!(count.kind, OpKind::X86Count { src, .. } if src == x86_gpr(3)));
    }
    for (index, op) in result.ops.iter().enumerate() {
        assert_eq!(op.id, OpId(index as u16));
    }
}

#[test]
fn apx_counts_strictly_lift_every_width_nf_state_and_source_class() {
    for (opcode, kind) in [
        (0x88, X86CountKind::Popcnt),
        (0xF4, X86CountKind::Tzcnt),
        (0xF5, X86CountKind::Lzcnt),
    ] {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            for nf in [false, true] {
                for memory in [false, true] {
                    let bytes = count_encoding(width, opcode, nf, memory);
                    let result = lift_single(&bytes).unwrap_or_else(|error| {
                        panic!("kind={kind:?} width={width:?} NF={nf} memory={memory}: {error:?}")
                    });
                    assert_guarded_count(&result, bytes.len(), width, kind, nf, memory);
                }
            }
        }
    }

    // SCALABLE gives W=1 precedence over an otherwise legal 66 pp value.
    let redundant_66 = [0x62, 0x74, 0xFD, 0x08, 0x88, 0xC3];
    let result = lift_single(&redundant_66).expect("POPCNT r8,rbx with W=1 and pp=66");
    assert_guarded_count(
        &result,
        redundant_66.len(),
        OpWidth::W64,
        X86CountKind::Popcnt,
        false,
        false,
    );
}

#[test]
fn apx_count_memory_uses_egpr_x4_segment_and_addr32_addressing() {
    // U=0 is X4=1 for a memory source and selects R16 as the SIB index.
    let egpr = [0x62, 0x74, 0xF8, 0x08, 0xF4, 0x44, 0x03, 0x20];
    let result = lift_single(&egpr).expect("TZCNT r8,[rbx+r16+32]");
    assert!(matches!(result.ops[0].kind, OpKind::X86RequireApx));
    assert!(matches!(
        &result.ops[1].kind,
        OpKind::Load {
            addr: Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 1,
                disp: 0x20,
                disp_size: DispSize::Disp8,
            },
            width: MemWidth::B8,
            ..
        } if *base == x86_gpr(3) && *index == x86_gpr(16)
    ));

    let fs = [0x64, 0x62, 0x74, 0xF8, 0x08, 0xF5, 0x44, 0x03, 0x20];
    let result = lift_single(&fs).expect("LZCNT r8,FS:[rbx+r16+32]");
    assert!(matches!(
        &result.ops[1].kind,
        OpKind::Load {
            addr: Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(base),
                index: Some(index),
                scale: 1,
                disp: 0x20,
            },
            ..
        } if *base == x86_gpr(3) && *index == x86_gpr(16)
    ));

    let addr32 = [0x67, 0x62, 0x74, 0x78, 0x08, 0x88, 0x44, 0x03, 0x20];
    let result = lift_single(&addr32).expect("POPCNT r8d,[ebx+r16d+32]");
    match &result.ops[1].kind {
        OpKind::Load {
            addr: Address::X86Addr32(inner),
            width: MemWidth::B4,
            ..
        } if matches!(
            inner.as_ref(),
            Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 1,
                disp: 0x20,
                disp_size: DispSize::Disp8,
            } if *base == x86_gpr(3) && *index == x86_gpr(16)
        ) => {}
        other => panic!("unexpected addr32 APX POPCNT load: {other:?}"),
    }
}

fn assert_count_ud(bytes: &[u8], name: &str) {
    let result = lift_single(bytes)
        .unwrap_or_else(|error| panic!("{name} must strictly lift to #UD: {error:?}"));
    assert_invalid_opcode_trap(&result, 6);
}

#[test]
fn apx_count_reserved_payloads_strictly_trap_at_modrm_without_tail_decode() {
    for (bytes, name) in [
        (&[0x62, 0x74, 0x7E, 0x08, 0x88, 0xC3][..], "F3 pp"),
        (&[0x62, 0x74, 0x7F, 0x08, 0xF4, 0xC3][..], "F2 pp"),
        (&[0x62, 0x74, 0x7C, 0x18, 0xF5, 0xC3][..], "ND"),
        (&[0x62, 0x74, 0x7C, 0x88, 0x88, 0xC3][..], "z"),
        (&[0x62, 0x74, 0x7C, 0x28, 0xF4, 0xC3][..], "LL"),
        (&[0x62, 0x74, 0x7C, 0x09, 0xF5, 0xC3][..], "payload bit 0"),
        (&[0x62, 0x74, 0x7C, 0x0A, 0x88, 0xC3][..], "payload bit 1"),
        (&[0x62, 0x74, 0x74, 0x08, 0xF4, 0xC3][..], "V3:0"),
        (&[0x62, 0x74, 0x7C, 0x00, 0xF5, 0xC3][..], "V4"),
        (&[0x62, 0x74, 0x78, 0x08, 0x88, 0xC3][..], "register U"),
    ] {
        assert_count_ud(bytes, name);
    }

    // Once the ModR/M byte proves an independent reserved field, no apparent
    // SIB or displacement tail is required to establish #UD.
    assert_count_ud(&[0x62, 0x74, 0x7C, 0x18, 0xF4, 0x84], "ND memory tail");

    for bytes in [
        &[0xF0, 0x62, 0x74, 0x7C, 0x08, 0x88, 0xC3][..],
        &[0x66, 0x62, 0x74, 0x7C, 0x08, 0xF4, 0xC3],
        &[0x48, 0x62, 0x74, 0x7C, 0x08, 0xF5, 0xC3],
    ] {
        let result = lift_single(bytes).expect("legacy prefix before APX count must be #UD");
        assert_invalid_opcode_trap(&result, 2);
    }
}

#[test]
fn apx_count_incomplete_lengths_are_absolute_and_do_not_hide_reserved_fields() {
    for opcode in [0x88, 0xF4, 0xF5] {
        for p2 in [0x08, 0x0C, 0x18] {
            let bytes = [0x62, 0x74, 0x7C, p2, opcode];
            assert!(matches!(
                lift_single(&bytes),
                Err(LiftError::Incomplete {
                    have: 5,
                    need: 6,
                    ..
                })
            ));
        }
    }
    let missing_sib = lift_single(&[0x62, 0x74, 0x7C, 0x08, 0x88, 0x84]);
    assert!(
        matches!(
            missing_sib,
            Err(LiftError::Incomplete {
                have: 6,
                need: 7,
                ..
            })
        ),
        "unexpected missing-SIB result: {missing_sib:?}"
    );
    assert!(matches!(
        lift_single(&[0x62, 0x74, 0x7C, 0x08, 0x88, 0x84, 0x03]),
        Err(LiftError::Incomplete {
            have: 7,
            need: 11,
            ..
        })
    ));
}

fn count_function(bytes: &[u8]) -> SmirFunction {
    let result = lift_single(bytes).expect("lift guarded APX count");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
    function.add_block(block);
    function
}

#[cfg(feature = "smir-jit")]
#[test]
fn apx_count_x86_jit_gate_admits_exact_register_and_helper_backed_memory_shapes() {
    use crate::smir::lower::runtime::{is_native_clobber_safe, is_native_clobber_safe_excluding};

    for opcode in [0x88, 0xF4, 0xF5] {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            for nf in [false, true] {
                for memory in [false, true] {
                    let bytes = count_encoding(width, opcode, nf, memory);
                    let mut function = count_function(&bytes);
                    let entry = function.entry;
                    function
                        .get_block_mut(entry)
                        .unwrap()
                        .set_terminator(Terminator::Return { values: vec![] });
                    let label =
                        format!("opcode={opcode:#04x} width={width:?} NF={nf} memory={memory}");

                    assert_eq!(
                        is_native_clobber_safe(&function),
                        !memory,
                        "{label}: memory-free admission"
                    );
                    assert!(
                        is_native_clobber_safe_excluding(
                            &function,
                            &std::collections::HashMap::new(),
                            true,
                        ),
                        "{label}: helper-backed memory admission"
                    );
                }
            }
        }
    }
}

#[test]
fn apx_count_guard_and_flags_survive_o2_and_are_noncommitting_when_disabled() {
    for (bytes, source, expected_result, expected_flags) in [
        (
            &[0x62, 0x74, 0xFC, 0x08, 0x88, 0xC3][..],
            0xF0_u64,
            4_u64,
            0x402_u64,
        ),
        (
            &[0x62, 0x74, 0xFC, 0x08, 0xF4, 0xC3][..],
            0_u64,
            64_u64,
            0xC97_u64,
        ),
        (
            &[0x62, 0x74, 0xFC, 0x0C, 0xF5, 0xC3][..],
            1_u64 << 63,
            0_u64,
            0xCD7_u64,
        ),
    ] {
        let original = count_function(bytes);
        let mut optimized = original.clone();
        crate::smir::optimize::optimize_function(
            &mut optimized,
            crate::smir::optimize::OptLevel::O2,
        );

        for function in [&original, &optimized] {
            assert!(matches!(
                function.entry_block().unwrap().ops.first(),
                Some(SmirOp {
                    kind: OpKind::X86RequireApx,
                    ..
                })
            ));
            for enabled in [false, true] {
                let mut context = SmirContext::new_x86_64();
                context.write_vreg(x86_gpr(3), source);
                context.write_vreg(x86_gpr(8), 0xA1B2_C3D4_E5F6_7788);
                context.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
                let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                    unreachable!()
                };
                x86.apx_enabled = enabled;
                let mut memory = FlatMemory::new(0x100);

                let execution = SmirInterpreter::new().execute_block(
                    &mut context,
                    &mut memory,
                    function.entry_block().unwrap(),
                );
                if enabled {
                    assert!(matches!(execution, BlockResult::Exit(ExitReason::Halt)));
                    assert_eq!(context.read_vreg(x86_gpr(8)), expected_result);
                    assert_eq!(context.flags.materialized.to_rflags(), expected_flags);
                } else {
                    assert!(matches!(
                        execution,
                        BlockResult::Exit(ExitReason::Undefined {
                            addr: 0x1000,
                            opcode: 0,
                        })
                    ));
                    assert_eq!(context.read_vreg(x86_gpr(8)), 0xA1B2_C3D4_E5F6_7788);
                    assert_eq!(context.flags.materialized.to_rflags(), 0xCD7);
                }
            }
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn apx_count_native_guard_is_dynamic_and_noncommitting() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    for (bytes, source, expected_result, expected_flags) in [
        (
            &[0x62, 0x74, 0xFC, 0x08, 0x88, 0xC3][..],
            0xF0_u64,
            4_u64,
            0x402_u64,
        ),
        (
            &[0x62, 0x74, 0xFC, 0x0C, 0xF5, 0xC3][..],
            1_u64 << 63,
            0_u64,
            0xCD7_u64,
        ),
    ] {
        let mut function = count_function(bytes);
        let entry = function.entry;
        function
            .get_block_mut(entry)
            .unwrap()
            .set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_jit_fault_deopt_guards(true);
        let lowered = lowerer.lower_function(&function).expect("lower APX count");
        let executable = ExecMem::new(&lowerer.finalize().unwrap()).unwrap();

        const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;
        for enabled in [false, true] {
            let mut registers = GuestRegs::default();
            registers.gpr[3] = source;
            registers.gpr[8] = 0xA1B2_C3D4_E5F6_7788;
            registers.rflags = 0xCD7;
            registers.apx_enabled = u64::from(enabled);
            registers.exit_pc = SENTINEL_PC;
            let initial = registers;

            executable.run(lowered.entry_offset, &mut registers);

            if enabled {
                assert_eq!(registers.gpr[8], expected_result);
                assert_eq!(registers.rflags & 0xCD7, expected_flags & 0xCD7);
                assert_eq!(registers.exit_pc, SENTINEL_PC);
            } else {
                assert_eq!(registers.gpr, initial.gpr);
                // The user-mode trampoline cannot import guest IF into host
                // RFLAGS; CPU integration preserves it in `interrupt_flags`.
                // Compare the materialized status/DF image owned by this path.
                assert_eq!(registers.rflags & 0xCD7, initial.rflags & 0xCD7);
                assert_eq!(registers.exit_pc, 0x1000);
            }
        }
    }
}
