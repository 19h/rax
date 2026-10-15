use super::*;

#[test]
fn overflowing_wide_instruction_is_rejected_before_continuation_fetch() {
    use rax::isa::ebpf::{EbpfDecodeError, EbpfMemFault, EbpfMemory};

    struct FetchMemory {
        reads: usize,
    }
    impl EbpfMemory for FetchMemory {
        fn read_bytes(&mut self, _: u64, out: &mut [u8]) -> Result<(), EbpfMemFault> {
            self.reads += 1;
            out.fill(0);
            if self.reads == 1 {
                out[0] = 0x18;
            }
            Ok(())
        }

        fn write_bytes(&mut self, addr: u64, bytes: &[u8]) -> Result<(), EbpfMemFault> {
            Err(EbpfMemFault {
                addr,
                size: bytes.len(),
                write: true,
            })
        }
    }
    let pc = u64::MAX - 15;
    let mut cpu = EbpfCpu::new(EbpfConfig {
        text_range: Some((pc, u64::MAX)),
        ..EbpfConfig::default()
    });
    cpu.regs.pc = pc;
    let mut memory = FetchMemory { reads: 0 };
    assert_eq!(
        cpu.fetch_and_decode(&mut memory),
        Err(EbpfTrap::DecodeError {
            pc,
            error: EbpfDecodeError::UnexpectedEof {
                needed: 16,
                available: 15,
            },
        })
    );
    assert_eq!(memory.reads, 1);
    assert_eq!(cpu.regs.pc, pc);
}

#[test]
fn overflowing_call_target_is_rejected_before_frame_mutation() {
    // An eight-byte instruction at 2^64 - 8 extends beyond the exclusive
    // text end u64::MAX. Saturating addition would incorrectly admit it.
    let target_pc = u64::MAX & !7;
    assert_eq!(target_pc.checked_add(8), None);
    for version in [SbpfVersion::V3, SbpfVersion::V4] {
        let config = EbpfConfig {
            text_range: Some((0, u64::MAX)),
            ..EbpfConfig::solana(version)
        };
        let program = [
            encode_insn(0x8d, 1, 0, 0, 0, Endian::Little),
            encode_insn(0x95, 0, 0, 0, 0, Endian::Little),
        ]
        .concat();
        let expected = EbpfTrap::CallOutsideTextSegment { pc: 0, target_pc };
        let mut direct = EbpfCpu::new(config.clone());
        direct.regs.r[1] = target_pc;
        let initial_regs = direct.regs.r;
        let mut memory = FlatMemory::new(program.len());
        memory.load(0, &program);
        assert_eq!(direct.run(&mut memory), EbpfExit::Trap(expected.clone()));
        assert_eq!(direct.regs.r, initial_regs);
        assert_eq!(direct.regs.pc, 0);
        assert_eq!(direct.insn_count, 1);

        for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut lifter = EbpfLifter::solana(version);
            let mut function = lifter
                .lift_function(
                    0,
                    &CodeSlice(&program),
                    &mut LiftContext::new(lifter.source_arch()),
                )
                .unwrap();
            optimize_function(&mut function, opt);
            let mut interpreter = SmirInterpreter::new();
            interpreter.add_function(function);
            let mut context = SmirContext::new_ebpf_with_config(config.clone());
            context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(1)), target_pc);
            assert!(matches!(
                interpreter.run(&mut context, &mut memory),
                ExitReason::EbpfTrap(actual) if actual == expected
            ));
            assert_eq!(context.pc, 0);
            assert_eq!(context.insn_count, 1);
            for (index, value) in initial_regs.iter().enumerate() {
                assert_eq!(
                    context.read_arch_reg(ArchReg::Ebpf(EbpfReg::R(index as u8))),
                    *value,
                    "{version:?} {opt:?} r{index}"
                );
            }
        }
    }
}

#[test]
fn caller_range_excludes_the_callee_function_body() {
    let program = [
        (0x85, 0, 1, 2),
        (0x07, 0, 0, 2),
        (0x95, 0, 0, 0),
        (0xb7, 0, 0, 40),
        (0x95, 0, 0, 0),
    ]
    .map(|(opc, dst, src, imm)| encode_insn(opc, dst, src, 0, imm, Endian::Little))
    .concat();
    let mut lifter = EbpfLifter::rfc9669();
    let function = lifter
        .lift_function(
            0,
            &CodeSlice(&program),
            &mut LiftContext::new(lifter.source_arch()),
        )
        .unwrap();
    assert_eq!(function.guest_range, (0, 24));
    let functions = lifter
        .lift_program(
            0,
            &CodeSlice(&program),
            &mut LiftContext::new(lifter.source_arch()),
        )
        .unwrap();
    let mut ranges = functions
        .iter()
        .map(|function| function.guest_range)
        .collect::<Vec<_>>();
    ranges.sort_unstable();
    assert_eq!(ranges, vec![(0, 24), (24, 40)]);
}

#[test]
fn guest_boundaries_are_observable_and_rejected_by_native_admission() {
    use rax::smir::ir::ops::{OpKind, SmirOp};
    use rax::smir::ir::types::OpId;
    let kind = OpKind::EbpfInstruction {
        dialect: EbpfDialect::Rfc9669,
        endian: Endian::Big,
        size: 8,
    };
    assert!(kind.has_side_effects());
    assert!(kind.source_vregs().is_empty());
    assert!(kind.dests().is_empty());
    assert!(!kind.is_jit_safe());
    assert!(!SmirOp::new(OpId(0), 0, kind).is_jit_safe());
    use rax::smir::lower::{SmirLowerer, aarch64::Aarch64Lowerer, x86_64::X86_64Lowerer};
    let program = encode_insn(0x95, 0, 0, 0, 0, Endian::Little);
    let mut lifter = EbpfLifter::rfc9669();
    let function = lifter
        .lift_function(
            0,
            &CodeSlice(&program),
            &mut LiftContext::new(lifter.source_arch()),
        )
        .unwrap();
    assert!(Aarch64Lowerer::default().lower_function(&function).is_err());
    assert!(X86_64Lowerer::default().lower_function(&function).is_err());
}

#[test]
fn nested_legacy_and_register_calls_preserve_state_at_all_optimization_levels() {
    for version in [
        SbpfVersion::V0,
        SbpfVersion::V1,
        SbpfVersion::V2,
        SbpfVersion::V3,
        SbpfVersion::V4,
    ] {
        for indirect in [false, true] {
            let config = EbpfConfig {
                max_call_depth: 4,
                stack_frame_size: 64,
                text_range: Some((0, 80)),
                ..EbpfConfig::solana(version)
            };
            let legacy = matches!(version, SbpfVersion::V0 | SbpfVersion::V1 | SbpfVersion::V2);
            let outer = if indirect {
                match version {
                    SbpfVersion::V0 | SbpfVersion::V1 => (0x8d, 0, 0, 3),
                    SbpfVersion::V2 => (0x8d, 0, 3, 0),
                    _ => (0x8d, 3, 0, 0),
                }
            } else {
                (
                    0x85,
                    0,
                    if legacy { 0 } else { 1 },
                    if legacy { 0x1234 } else { 2 },
                )
            };
            let mut program = Vec::new();
            for (opc, dst, src, imm) in [
                outer,
                (0x07, 0, 0, 2),
                (0x95, 0, 0, 0),
                (0xb7, 6, 0, 99),
                (
                    0x85,
                    0,
                    if legacy { 0 } else { 1 },
                    if legacy { 0x5678 } else { 2 },
                ),
                (0x95, 0, 0, 0),
                (0x95, 0, 0, 0),
                (0xb7, 0, 0, 40),
                (0xb7, 6, 0, 999),
                (0x95, 0, 0, 0),
            ] {
                program.extend_from_slice(&encode_insn(opc, dst, src, 0, imm, Endian::Little));
            }
            let mut direct = EbpfCpu::new(config.clone());
            // Reference CALLX rounds the byte address down to an instruction slot.
            direct.regs.r[3] = 27;
            direct.regs.r[6..10].copy_from_slice(&[6, 7, 8, 9]);
            let initial = direct.regs.r;
            direct.register_function(0x1234, 24);
            direct.register_function(0x5678, 56);
            let mut mem = FlatMemory::new(128);
            mem.load(0, &program);
            assert_eq!(direct.run(&mut mem), EbpfExit::Exit { r0: 42 });
            assert_eq!(direct.insn_count, 9);
            for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                let mut lifter = EbpfLifter::solana(version);
                lifter.register_function(0x1234, 24);
                lifter.register_function(0x5678, 56);
                let functions = lifter
                    .lift_program(
                        0,
                        &CodeSlice(&program),
                        &mut LiftContext::new(lifter.source_arch()),
                    )
                    .unwrap();
                let mut interpreter = SmirInterpreter::new();
                for mut function in functions {
                    optimize_function(&mut function, opt);
                    interpreter.add_function(function);
                }
                let mut context = SmirContext::new_ebpf_with_config(config.clone());
                for (i, value) in initial.iter().enumerate() {
                    context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(i as u8)), *value);
                }
                assert!(
                    matches!(
                        interpreter.run(&mut context, &mut FlatMemory::new(128)),
                        ExitReason::Return { to: 42 }
                    ),
                    "{version:?} {indirect} {opt:?}"
                );
                for (i, value) in direct.regs.r.iter().enumerate() {
                    assert_eq!(
                        context.read_arch_reg(ArchReg::Ebpf(EbpfReg::R(i as u8))),
                        *value,
                        "{version:?} {indirect} {opt:?} r{i}"
                    );
                }
                assert_eq!(context.insn_count, direct.insn_count);
                assert_eq!(context.pc, direct.regs.pc);
            }
        }
    }
}

#[test]
fn instruction_limits_and_fault_pcs_survive_optimization() {
    let program = [
        (0xb7, 1, 0, 1),
        (0xb7, 1, 0, 2),
        (0x79, 0, 2, 0),
        (0x95, 0, 0, 0),
    ]
    .map(|(opc, dst, src, imm)| encode_insn(opc, dst, src, 0, imm, Endian::Little))
    .concat();
    for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut lifter = EbpfLifter::rfc9669();
        let mut function = lifter
            .lift_function(
                0,
                &CodeSlice(&program),
                &mut LiftContext::new(lifter.source_arch()),
            )
            .unwrap();
        optimize_function(&mut function, opt);
        let mut interpreter = SmirInterpreter::new();
        interpreter.add_function(function);
        interpreter.set_max_insns(2);
        let mut context = SmirContext::new_ebpf();
        context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(2)), 0x1000);
        assert!(matches!(
            interpreter.run(&mut context, &mut FlatMemory::new(32)),
            ExitReason::InsnLimit
        ));
        assert_eq!(context.pc, 16);
        assert_eq!(context.insn_count, 2);
        assert_eq!(context.read_arch_reg(ArchReg::Ebpf(EbpfReg::R(1))), 2);
        assert!(matches!(
            interpreter.run(&mut context, &mut FlatMemory::new(32)),
            ExitReason::MemoryFault {
                addr: 0x1000,
                write: false
            }
        ));
        assert_eq!(context.pc, 16);
    }
}

#[test]
fn recursive_calls_stop_at_the_same_precise_depth() {
    let program = [
        encode_insn(0x85, 0, 1, 0, -1, Endian::Little),
        encode_insn(0x95, 0, 0, 0, 0, Endian::Little),
    ]
    .concat();
    let config = EbpfConfig {
        max_call_depth: 4,
        text_range: Some((0, 16)),
        ..EbpfConfig::default()
    };
    let mut direct = EbpfCpu::new(config.clone());
    let mut memory = FlatMemory::new(16);
    memory.load(0, &program);
    assert_eq!(
        direct.run(&mut memory),
        EbpfExit::Trap(EbpfTrap::CallDepthExceeded { pc: 0 })
    );
    let mut lifter = EbpfLifter::rfc9669();
    let function = lifter
        .lift_function(
            0,
            &CodeSlice(&program),
            &mut LiftContext::new(lifter.source_arch()),
        )
        .unwrap();
    let mut interpreter = SmirInterpreter::new();
    interpreter.add_function(function);
    let mut context = SmirContext::new_ebpf_with_config(config);
    assert!(matches!(
        interpreter.run(&mut context, &mut memory),
        ExitReason::EbpfTrap(EbpfTrap::CallDepthExceeded { pc: 0 })
    ));
    assert_eq!(context.insn_count, direct.insn_count);
    assert_eq!(
        context.read_arch_reg(ArchReg::Ebpf(EbpfReg::R(10))),
        direct.regs.r[10]
    );
}

#[test]
fn local_calls_resume_the_caller_with_r0_42() {
    for dialect in [
        EbpfDialect::Rfc9669,
        EbpfDialect::Solana(SbpfVersion::V3),
        EbpfDialect::Solana(SbpfVersion::V4),
    ] {
        for endian in [Endian::Little, Endian::Big] {
            let mut program = Vec::new();
            for (opc, dst, src, imm) in [
                (0x85, 0, 1, 2),
                (0x07, 0, 0, 2),
                (0x95, 0, 0, 0),
                (0xb7, 0, 0, 40),
                (0x95, 0, 0, 0),
            ] {
                program.extend_from_slice(&encode_insn(opc, dst, src, 0, imm, endian));
            }
            let regs = [0; 11];
            let (out, _) = run_direct(&program, dialect, endian, &regs, &[]);
            assert!(matches!(out, NormalizedOutcome::Exited { r0: 42, .. }));
            assert_direct_smir_parity("local_call", &program, dialect, endian, &regs, &[]);
        }
    }
}

#[test]
fn function_range_uses_consumed_guest_bytes_including_wide_and_empty_ir_instructions() {
    for program in [
        [
            encode_insn(0x37, 0, 0, 1, -1, Endian::Little),
            encode_insn(0x95, 0, 0, 0, 0, Endian::Little),
        ]
        .concat(),
        [
            encode_lddw(0, 42, Endian::Little).to_vec(),
            encode_insn(0x95, 0, 0, 0, 0, Endian::Little).to_vec(),
        ]
        .concat(),
        encode_insn(0x95, 0, 0, 0, 0, Endian::Little).to_vec(),
    ] {
        let mut lifter = EbpfLifter::rfc9669();
        let function = lifter
            .lift_function(
                0,
                &CodeSlice(&program),
                &mut LiftContext::new(lifter.source_arch()),
            )
            .unwrap();
        assert_eq!(function.guest_range, (0, program.len() as u64));
        let mut interpreter = SmirInterpreter::new();
        interpreter.add_function(function);
        let mut context = SmirContext::new_ebpf();
        assert!(matches!(
            interpreter.run(&mut context, &mut FlatMemory::new(32)),
            ExitReason::Return { .. }
        ));
    }
}

#[test]
fn syscall_exit_advances_pc_to_a_resumable_continuation() {
    let program = [
        encode_insn(0x85, 0, 0, 0, 123, Endian::Little),
        encode_insn(0x07, 0, 0, 0, 2, Endian::Little),
        encode_insn(0x95, 0, 0, 0, 0, Endian::Little),
    ]
    .concat();
    let mut lifter = EbpfLifter::rfc9669();
    let function = lifter
        .lift_function(
            0,
            &CodeSlice(&program),
            &mut LiftContext::new(lifter.source_arch()),
        )
        .unwrap();
    let mut interpreter = SmirInterpreter::new();
    for block in function.blocks {
        interpreter.add_block(block.guest_pc, block);
    }
    let mut context = SmirContext::new_ebpf();
    let mut memory = FlatMemory::new(32);
    assert!(matches!(
        interpreter.run(&mut context, &mut memory),
        ExitReason::Syscall { num: 123, .. }
    ));
    assert_eq!(context.pc, 8);
    context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(0)), 40);
    assert!(matches!(
        interpreter.run(&mut context, &mut memory),
        ExitReason::Return { to: 42 }
    ));
    assert_eq!(context.pc, 24);
}
