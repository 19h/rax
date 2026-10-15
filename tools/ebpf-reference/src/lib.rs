//! Independent execution against the pinned Anza interpreter. Run this package
//! separately; it is not a substitute for the engine's portable regression suite.

#[cfg(test)]
mod tests {
    use std::ptr::NonNull;
    use std::sync::Arc;

    use rax::isa::ebpf::{
        EbpfConfig, EbpfCpu, EbpfExit, MM_BYTECODE_START, SbpfMemoryMapping, SbpfVersion,
    };
    use rax::smir::ir::types::{ArchReg, EbpfReg};
    use rax::smir::lift::{LiftContext, SmirLifter};
    use rax::smir::optimize::{OptLevel, optimize_function};
    use rax::smir::{EbpfLifter, ExitReason, SmirContext, SmirInterpreter};
    use solana_sbpf::{
        aligned_memory::AlignedMemory,
        ebpf,
        elf::Executable,
        memory_region::{MemoryMapping, MemoryRegion},
        program::{BuiltinProgram, FunctionRegistry, SBPFVersion},
        verifier::RequisiteVerifier,
        vm::{CallFrame, Config, ContextObject, EbpfVm, ExecutionMode},
    };

    struct Context {
        remaining: u64,
        mapping: MemoryMapping,
    }
    impl ContextObject for Context {
        fn consume(&mut self, amount: u64) {
            self.remaining = self.remaining.saturating_sub(amount);
        }
        fn get_remaining(&self) -> u64 {
            self.remaining
        }
        fn active_mapping_ptr(&mut self) -> NonNull<MemoryMapping> {
            NonNull::from(&mut self.mapping)
        }
    }

    fn versions() -> [(SbpfVersion, SBPFVersion); 5] {
        [
            (SbpfVersion::V0, SBPFVersion::V0),
            (SbpfVersion::V1, SBPFVersion::V1),
            (SbpfVersion::V2, SBPFVersion::V2),
            (SbpfVersion::V3, SBPFVersion::V3),
            (SbpfVersion::V4, SBPFVersion::V4),
        ]
    }

    fn instruction(opcode: u8, dst: u8, src: u8, imm: i32) -> [u8; 8] {
        let value = imm.to_le_bytes();
        [
            opcode,
            src << 4 | dst,
            0,
            0,
            value[0],
            value[1],
            value[2],
            value[3],
        ]
    }

    fn reference(
        program: &[u8],
        version: SBPFVersion,
        indirect_target: u64,
    ) -> Result<[u64; 11], String> {
        let config = Config {
            max_call_depth: 4,
            ..Config::default()
        };
        let loader = Arc::new(BuiltinProgram::<Context>::new_loader(config));
        let mut registry = FunctionRegistry::default();
        registry
            .register_function(1234, b"first".to_vec(), 3)
            .unwrap();
        registry
            .register_function(5678, b"second".to_vec(), 10)
            .unwrap();
        let executable =
            Executable::from_text_bytes(program, loader.clone(), version, registry).unwrap();
        executable.verify::<RequisiteVerifier>().unwrap();
        let config = executable.get_config();
        let mut stack = AlignedMemory::<{ ebpf::HOST_ALIGN }>::zero_filled(config.stack_size());
        let stack_len = stack.len();
        let stack_region = if version.stack_frame_gaps() && config.enable_stack_frame_gaps {
            MemoryRegion::new_gapped(
                &mut stack,
                ebpf::MM_STACK_START,
                config.stack_frame_size as u64,
            )
        } else {
            MemoryRegion::new(&mut stack, ebpf::MM_STACK_START)
        };
        let regions = vec![executable.get_ro_region(), stack_region];
        // SAFETY: the executable's immutable bytes and the stack allocation stay
        // live and stable until after VM and mapping destruction. Writable bytes
        // are exclusively owned plain u8 stack storage; no host access occurs
        // during guest execution. Regions do not overlap or alias, execution is
        // synchronous and interpreted, and no callbacks/re-entry/FFI unwinding
        // occur. The VM borrow keeps Context and its mapping at stable addresses.
        let mapping = unsafe { MemoryMapping::new(regions, config, version) }.unwrap();
        let mut context = Context {
            remaining: 10000,
            mapping,
        };
        let mut vm = EbpfVm::new(loader, version, &mut context, stack_len);
        vm.registers[3] = indirect_target;
        vm.registers[6..10].copy_from_slice(&[6, 7, 8, 9]);
        let mut frames = vec![CallFrame::default(); config.max_call_depth];
        let (_, result) =
            vm.execute_program(&executable, &mut ExecutionMode::Interpreted, &mut frames);
        let result: Result<u64, _> = result.into();
        let returned = result.map_err(|e| format!("{e:?}"))?;
        // The interpreted VM register buffer is not updated on exit. Tracer
        // snapshots are taken before each instruction; successful root EXIT
        // changes no GPR, so its snapshot is the final architectural GPR state.
        let final_trace = vm.register_trace.last().unwrap();
        assert_eq!(program[final_trace[11] as usize * 8], 0x95);
        assert_eq!(vm.call_depth, 0);
        assert_eq!(final_trace[0], returned);
        Ok(final_trace[..11].try_into().unwrap())
    }

    fn rax_config(version: SbpfVersion, size: usize) -> EbpfConfig {
        EbpfConfig {
            max_call_depth: 4,
            max_instructions: Some(10000),
            text_range: Some((MM_BYTECODE_START, MM_BYTECODE_START + size as u64)),
            ..EbpfConfig::solana(version)
        }
    }

    fn mapping(program: &[u8], config: &EbpfConfig) -> SbpfMemoryMapping {
        let address = if config.dialect.sbpf_version().unwrap().static_syscalls() {
            0
        } else {
            MM_BYTECODE_START
        };
        SbpfMemoryMapping::new_solana_with_config(
            config,
            program,
            address,
            program,
            4 * 4096,
            0,
            &[],
        )
        .unwrap()
    }

    fn read_program(version: SbpfVersion, address: u64, write: bool) -> Vec<u8> {
        let mut program = if version == SbpfVersion::V2 {
            [
                instruction(0xb7, 1, 0, (address >> 32) as i32),
                instruction(0x67, 1, 0, 32),
                instruction(0x47, 1, 0, address as u32 as i32),
            ]
            .concat()
        } else {
            [
                instruction(0x18, 1, 0, address as u32 as i32),
                instruction(0, 0, 0, (address >> 32) as i32),
            ]
            .concat()
        };
        program.extend_from_slice(&if write {
            instruction(
                if version == SbpfVersion::V2 {
                    0x27
                } else {
                    0x72
                },
                1,
                0,
                7,
            )
        } else {
            instruction(
                if version == SbpfVersion::V2 {
                    0x2c
                } else {
                    0x71
                },
                0,
                1,
                0,
            )
        });
        program.extend_from_slice(&instruction(0x95, 0, 0, 0));
        program
    }

    #[test]
    fn versioned_data_addresses_gaps_and_permissions_match_pinned_anza() {
        for (version, upstream) in versions() {
            let rodata = if matches!(version, SbpfVersion::V3 | SbpfVersion::V4) {
                0
            } else {
                MM_BYTECODE_START
            };
            for (address, write) in [
                (rodata, false),
                (rodata, true),
                (ebpf::MM_STACK_START + 4096, false),
            ] {
                let program = read_program(version, address, write);
                let expected = reference(&program, upstream, 0);
                let config = rax_config(version, program.len());
                let mut cpu = EbpfCpu::new(config.clone());
                cpu.regs.pc = MM_BYTECODE_START;
                cpu.regs.r[6..10].copy_from_slice(&[6, 7, 8, 9]);
                let actual = cpu.run(&mut mapping(&program, &config));
                assert_eq!(
                    matches!(actual, EbpfExit::Exit { .. }),
                    expected.is_ok(),
                    "{version:?} address={address:#x} write={write}: {actual:?} vs {expected:?}"
                );
                if let Ok(registers) = expected {
                    assert_eq!(cpu.regs.r, registers);
                }
                eprintln!("executed upstream {upstream:?} data address={address:#x} write={write}");
            }
        }
    }

    #[test]
    fn lower_rodata_versions_do_not_expose_a_program_data_alias() {
        for (version, upstream) in [
            (SbpfVersion::V3, SBPFVersion::V3),
            (SbpfVersion::V4, SBPFVersion::V4),
        ] {
            let program = read_program(version, MM_BYTECODE_START, false);
            let expected = reference(&program, upstream, 0);
            assert!(expected.is_err());
            let config = rax_config(version, program.len());
            let mut cpu = EbpfCpu::new(config.clone());
            cpu.regs.pc = MM_BYTECODE_START;
            assert!(
                matches!(cpu.run(&mut mapping(&program, &config)), EbpfExit::Trap(_)),
                "{version:?}: upstream {expected:?}"
            );
        }
    }

    #[test]
    fn v0_cross_frame_access_matches_the_reference_translation() {
        for offset in [4094, 4096, 28670] {
            let mut program = read_program(SbpfVersion::V0, ebpf::MM_STACK_START + offset, false);
            let load_offset = program.len() - 16;
            program[load_offset] = 0x61; // LDXW, four bytes from the selected address.
            let expected = reference(&program, SBPFVersion::V0, 0);
            eprintln!("upstream V0 stack offset={offset} cross-frame={expected:?}");
            let config = rax_config(SbpfVersion::V0, program.len());
            let mut cpu = EbpfCpu::new(config.clone());
            cpu.regs.pc = MM_BYTECODE_START;
            cpu.regs.r[6..10].copy_from_slice(&[6, 7, 8, 9]);
            let actual = cpu.run(&mut mapping(&program, &config));
            assert_eq!(
                matches!(actual, EbpfExit::Exit { .. }),
                expected.is_ok(),
                "offset={offset}: {actual:?} vs {expected:?}"
            );
            if let Ok(registers) = expected {
                assert_eq!(cpu.regs.r, registers);
            }
        }
    }

    #[test]
    fn nested_frames_and_both_call_forms_match_pinned_anza_execution() {
        for (version, reference_version) in versions() {
            for indirect in [false, true] {
                let legacy = matches!(version, SbpfVersion::V0 | SbpfVersion::V1 | SbpfVersion::V2);
                let first = if indirect {
                    match version {
                        SbpfVersion::V0 | SbpfVersion::V1 => instruction(0x8d, 0, 0, 3),
                        SbpfVersion::V2 => instruction(0x8d, 0, 3, 0),
                        _ => instruction(0x8d, 3, 0, 0),
                    }
                } else {
                    instruction(
                        0x85,
                        0,
                        if legacy { 0 } else { 1 },
                        if legacy { 1234 } else { 2 },
                    )
                };
                let program = [
                    first,
                    instruction(0x07, 0, 0, 2),
                    instruction(0x95, 0, 0, 0),
                    instruction(0xb7, 6, 0, 60),
                    instruction(0xb7, 7, 0, 70),
                    instruction(0xb7, 8, 0, 80),
                    instruction(0xb7, 9, 0, 90),
                    instruction(
                        0x85,
                        0,
                        if legacy { 0 } else { 1 },
                        if legacy { 5678 } else { 2 },
                    ),
                    instruction(0x95, 0, 0, 0),
                    instruction(0x95, 0, 0, 0),
                    instruction(0xb7, 6, 0, 600),
                    instruction(0xb7, 7, 0, 700),
                    instruction(0xb7, 8, 0, 800),
                    instruction(0xb7, 9, 0, 900),
                    instruction(0xbf, 0, 10, 0),
                    instruction(0x95, 0, 0, 0),
                ]
                .concat();
                let target = MM_BYTECODE_START + 24;
                let expected = reference(&program, reference_version, target).unwrap();
                let config = rax_config(version, program.len());
                let mut direct = EbpfCpu::new(config.clone());
                direct.regs.pc = MM_BYTECODE_START;
                direct.regs.r[3] = target;
                direct.regs.r[6..10].copy_from_slice(&[6, 7, 8, 9]);
                direct.register_function(1234, MM_BYTECODE_START + 24);
                direct.register_function(5678, MM_BYTECODE_START + 80);
                assert!(matches!(
                    direct.run(&mut mapping(&program, &config)),
                    EbpfExit::Exit { .. }
                ));
                assert_eq!(
                    direct.regs.r, expected,
                    "direct {version:?} indirect={indirect}"
                );
                for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                    let mut lifter = EbpfLifter::solana(version);
                    lifter.register_function(1234, MM_BYTECODE_START + 24);
                    lifter.register_function(5678, MM_BYTECODE_START + 80);
                    let mut memory = mapping(&program, &config);
                    let functions = lifter
                        .lift_program(
                            MM_BYTECODE_START,
                            &memory,
                            &mut LiftContext::new(lifter.source_arch()),
                        )
                        .unwrap();
                    let mut interp = SmirInterpreter::new();
                    for mut function in functions {
                        optimize_function(&mut function, opt);
                        interp.add_function(function);
                    }
                    let mut context = SmirContext::new_ebpf_with_config(config.clone());
                    context.pc = MM_BYTECODE_START;
                    context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(3)), target);
                    for index in 6..=9 {
                        context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(index)), u64::from(index));
                    }
                    assert!(matches!(
                        interp.run(&mut context, &mut memory),
                        ExitReason::Return { .. }
                    ));
                    for (i, value) in expected.iter().enumerate() {
                        assert_eq!(
                            context.read_arch_reg(ArchReg::Ebpf(EbpfReg::R(i as u8))),
                            *value,
                            "{version:?} {indirect} {opt:?} r{i}"
                        );
                    }
                }
                eprintln!(
                    "executed upstream SBPF {reference_version:?}, indirect={indirect}, r0={:#x}",
                    expected[0]
                );
            }
        }
    }
}
