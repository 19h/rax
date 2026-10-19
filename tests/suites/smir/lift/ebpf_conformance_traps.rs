//! Precise SMIR trap parity with the direct CPU.
//!
//! The pinned Anza differential (`tools/ebpf-reference`) found that SMIR
//! reported a configured instruction-meter exhaustion as the resumable
//! interpreter slice (`ExitReason::InsnLimit`), and guest memory faults as a
//! generic `ExitReason::MemoryFault` whose address came from the backend (the
//! end of the access for `FlatMemory`). The direct CPU and the reference both
//! report a fatal trap with the faulting instruction, the effective start
//! address, the access width, and its direction. These tests require the
//! identical `EbpfTrap`, PC, and instruction count on every plane.

use super::*;
use rax::isa::ebpf::{EbpfHelperKey, EbpfMemFault};

fn smir_trap(
    program: &[u8],
    config: &EbpfConfig,
    regs: &[(u8, u64)],
    memory_len: usize,
    opt: OptLevel,
) -> (ExitReason, u64, u64) {
    let mut lifter = EbpfLifter::new(config.dialect, config.endian);
    lifter.platform = config.platform.clone();
    let mut lift_context = LiftContext::new(lifter.source_arch());
    lift_context.endian = config.endian;
    let functions = lifter
        .lift_program(0, &CodeSlice(program), &mut lift_context)
        .unwrap();
    let mut interpreter = SmirInterpreter::new();
    interpreter.set_max_insns(1_000_000);
    for mut function in functions {
        optimize_function(&mut function, opt);
        interpreter.add_function(function);
    }
    let mut memory = FlatMemory::new(memory_len);
    memory.load(0, program);
    let mut context = SmirContext::new_ebpf_with_config(config.clone());
    for &(reg, value) in regs {
        context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(reg)), value);
    }
    let reason = interpreter.run(&mut context, &mut memory);
    (reason, context.insn_count, context.pc)
}

fn direct_trap(
    program: &[u8],
    config: &EbpfConfig,
    regs: &[(u8, u64)],
    memory_len: usize,
) -> (EbpfExit, u64, u64) {
    let mut memory = FlatMemory::new(memory_len);
    memory.load(0, program);
    let mut cpu = EbpfCpu::new(config.clone());
    for &(reg, value) in regs {
        cpu.regs.r[usize::from(reg)] = value;
    }
    let exit = cpu.run(&mut memory);
    (exit, cpu.insn_count, cpu.regs.pc)
}

fn dialects() -> [(EbpfDialect, Endian); 4] {
    [
        (EbpfDialect::Rfc9669, Endian::Little),
        (EbpfDialect::Rfc9669, Endian::Big),
        (EbpfDialect::Solana(SbpfVersion::V0), Endian::Little),
        (EbpfDialect::Solana(SbpfVersion::V3), Endian::Little),
    ]
}

#[test]
fn instruction_meter_exhaustion_is_the_same_fatal_trap_on_every_plane() {
    for (dialect, endian) in dialects() {
        // r0 = 1; loop: r0 += 1; ja loop (backward); exit
        let program = [
            encode_insn(0xb7, 0, 0, 0, 1, endian),
            encode_insn(0x07, 0, 0, 0, 1, endian),
            encode_insn(0x05, 0, 0, -2, 0, endian),
            encode_insn(0x95, 0, 0, 0, 0, endian),
        ]
        .concat();
        for budget in 0..=7u64 {
            let config = EbpfConfig {
                dialect,
                endian,
                max_instructions: Some(budget),
                ..EbpfConfig::default()
            };
            let (exit, count, pc) = direct_trap(&program, &config, &[], 256);
            let EbpfExit::Trap(expected @ EbpfTrap::ExceededMaxInstructions { .. }) = exit else {
                panic!("{dialect:?} budget={budget}: direct {exit:?}");
            };
            assert_eq!(count, budget);
            for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                let (reason, smir_count, smir_pc) = smir_trap(&program, &config, &[], 256, opt);
                assert!(
                    matches!(reason, ExitReason::EbpfTrap(trap) if trap == expected),
                    "{dialect:?} {endian:?} budget={budget} {opt:?}: {reason:?} vs {expected:?}"
                );
                assert_eq!((smir_count, smir_pc), (count, pc), "{dialect:?} {opt:?}");
            }
        }
    }
}

#[test]
fn guest_memory_faults_report_the_direct_cpu_trap_on_every_plane() {
    const MEMORY_LEN: usize = 0x1000;
    // (opcode, imm, access bytes, uses r0 as CMPXCHG comparand)
    let accesses: &[(u8, i32, u64)] = &[
        (0x71, 0, 1),    // LDXB
        (0x69, 0, 2),    // LDXH
        (0x61, 0, 4),    // LDXW
        (0x79, 0, 8),    // LDXDW
        (0x72, 7, 1),    // STB
        (0x6a, 7, 2),    // STH
        (0x62, 7, 4),    // STW
        (0x7a, 7, 8),    // STDW
        (0x73, 0, 1),    // STXB
        (0x6b, 0, 2),    // STXH
        (0x63, 0, 4),    // STXW
        (0x7b, 0, 8),    // STXDW
        (0xc3, 0x01, 4), // atomic fetch-add, 32-bit
        (0xdb, 0x51, 8), // atomic fetch-and, 64-bit
        (0xdb, 0xe1, 8), // XCHG, 64-bit
        (0xc3, 0xf1, 4), // CMPXCHG, 32-bit
    ];
    for (dialect, endian) in dialects() {
        for &(opc, imm, bytes) in accesses {
            let atomic = matches!(opc, 0xc3 | 0xdb);
            if atomic && dialect.is_solana() {
                continue;
            }
            let rfc_imm = if opc & 0x07 == 0x02 || atomic { imm } else { 0 };
            // Bases at the end, straddling the end by one byte (multi-byte
            // accesses), and near the top of the address space.
            let mut bases = vec![MEMORY_LEN as u64, u64::MAX - 2];
            if bytes > 1 {
                bases.push(MEMORY_LEN as u64 - bytes + 1);
            }
            for base in bases {
                // LDX: dst <- [src]; ST: [dst] <- imm with src = 0; STX and
                // atomics: [dst] <- src.
                let (dst, src) = match opc & 0x07 {
                    0x01 => (0, 2),
                    0x02 => (2, 0),
                    _ => (2, 3),
                };
                let program = [
                    encode_insn(0xb7, 4, 0, 0, 9, endian),
                    encode_insn(opc, dst, src, 0, rfc_imm, endian),
                    encode_insn(0x95, 0, 0, 0, 0, endian),
                ]
                .concat();
                let config = EbpfConfig {
                    dialect,
                    endian,
                    ..EbpfConfig::default()
                };
                let regs = [(2, base), (3, 0x1122_3344_5566_7788)];
                let (exit, count, pc) = direct_trap(&program, &config, &regs, MEMORY_LEN);
                let write = opc & 0x07 != 0x01;
                let expected = EbpfTrap::MemoryFault {
                    pc: 8,
                    fault: EbpfMemFault {
                        addr: base,
                        size: bytes as usize,
                        write,
                    },
                };
                assert_eq!(
                    exit,
                    EbpfExit::Trap(expected),
                    "{dialect:?} {opc:#x} {base:#x}"
                );
                assert_eq!((count, pc), (2, 8));
                for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                    let (reason, smir_count, smir_pc) =
                        smir_trap(&program, &config, &regs, MEMORY_LEN, opt);
                    assert!(
                        matches!(reason, ExitReason::EbpfTrap(trap) if trap == expected),
                        "{dialect:?} {endian:?} {opc:#x} base={base:#x} {opt:?}: {reason:?}"
                    );
                    assert_eq!(
                        (smir_count, smir_pc),
                        (2, 8),
                        "{dialect:?} {opc:#x} {opt:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn optimization_keeps_a_trapping_division_whose_result_is_dead() {
    // SBPF DIV32_REG / UDIV64_REG / SREM64_REG by zero, followed by a
    // self-loop that never reads the destination: every plane must trap at
    // the division (the pinned Anza interpreter raises DivideByZero there).
    for (version, opc) in [
        (SbpfVersion::V0, 0x3c),
        (SbpfVersion::V2, 0x5e),
        (SbpfVersion::V2, 0xfe),
        (SbpfVersion::V3, 0x9f),
    ] {
        let endian = Endian::Little;
        let program = [
            encode_insn(opc, 8, 9, 0, 0, endian),
            encode_insn(0x05, 0, 0, -1, 0, endian),
            encode_insn(0x95, 0, 0, 0, 0, endian),
        ]
        .concat();
        let config = EbpfConfig {
            max_instructions: Some(50),
            ..EbpfConfig::solana(version)
        };
        let regs = [(8, 0x1234), (9, 0)];
        let (exit, count, pc) = direct_trap(&program, &config, &regs, 256);
        assert_eq!(exit, EbpfExit::Trap(EbpfTrap::DivideByZero { pc: 0 }));
        assert_eq!((count, pc), (1, 0));
        for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let (reason, smir_count, smir_pc) = smir_trap(&program, &config, &regs, 256, opt);
            assert!(
                matches!(
                    reason,
                    ExitReason::EbpfTrap(EbpfTrap::DivideByZero { pc: 0 })
                ),
                "{version:?} {opc:#x} {opt:?}: {reason:?}"
            );
            assert_eq!(
                (smir_count, smir_pc),
                (1, 0),
                "{version:?} {opc:#x} {opt:?}"
            );
        }
    }
}

/// Every plane over an SBPF executable mapping with the text range set.
fn sbpf_planes(
    version: SbpfVersion,
    program: &[u8],
    regs: &[(u8, u64)],
) -> Vec<(&'static str, Result<u64, EbpfTrap>, u64, usize, u64)> {
    use rax::isa::ebpf::{MM_BYTECODE_START, SbpfMemoryMapping};
    let config = EbpfConfig {
        max_call_depth: 4,
        text_range: Some((MM_BYTECODE_START, MM_BYTECODE_START + program.len() as u64)),
        ..EbpfConfig::solana(version)
    };
    let mapping = || {
        SbpfMemoryMapping::new_solana_with_config(
            &config,
            program,
            if version.enable_lower_rodata_vaddr() {
                0
            } else {
                MM_BYTECODE_START
            },
            &[],
            4 * 4096,
            0,
            &[],
        )
        .unwrap()
    };
    let mut planes = Vec::new();
    let mut cpu = EbpfCpu::new(config.clone());
    cpu.regs.pc = MM_BYTECODE_START;
    for &(reg, value) in regs {
        cpu.regs.r[usize::from(reg)] = value;
    }
    let exit = match cpu.run(&mut mapping()) {
        EbpfExit::Exit { r0 } => Ok(r0),
        EbpfExit::Trap(trap) => Err(trap),
        other => panic!("{other:?}"),
    };
    planes.push((
        "direct",
        exit,
        cpu.insn_count,
        cpu.call_frames.len(),
        cpu.regs.r[10],
    ));
    for (name, opt) in [
        ("smir-o0", OptLevel::O0),
        ("smir-o1", OptLevel::O1),
        ("smir-o2", OptLevel::O2),
    ] {
        let mut memory = mapping();
        let mut lifter = EbpfLifter::solana(version);
        let mut interpreter = SmirInterpreter::new();
        for mut function in lifter
            .lift_program(
                MM_BYTECODE_START,
                &memory,
                &mut LiftContext::new(lifter.source_arch()),
            )
            .unwrap()
        {
            optimize_function(&mut function, opt);
            interpreter.add_function(function);
        }
        let mut context = SmirContext::new_ebpf_with_config(config.clone());
        context.pc = MM_BYTECODE_START;
        for &(reg, value) in regs {
            context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(reg)), value);
        }
        let exit = match interpreter.run(&mut context, &mut memory) {
            ExitReason::Return { to } => Ok(to),
            ExitReason::EbpfTrap(trap) => Err(trap),
            other => panic!("{name}: {other:?}"),
        };
        let rax::smir::ir::context::ArchRegState::Ebpf(state) = &context.arch_regs else {
            unreachable!("eBPF context");
        };
        let frames = state.call_frames.len();
        planes.push((
            name,
            exit,
            context.insn_count,
            frames,
            context.read_arch_reg(ArchReg::Ebpf(EbpfReg::R(10))),
        ));
    }
    planes
}

fn assert_planes(
    label: &str,
    planes: &[(&'static str, Result<u64, EbpfTrap>, u64, usize, u64)],
    expected: Result<u64, EbpfTrap>,
    instructions: u64,
) {
    for (plane, exit, count, _, _) in planes {
        assert_eq!(*exit, expected, "{label} {plane}");
        assert_eq!(*count, instructions, "{label} {plane} instruction count");
    }
    let (_, _, _, frames, fp) = planes[0];
    for (plane, _, _, plane_frames, plane_fp) in planes {
        assert_eq!(
            (*plane_frames, *plane_fp),
            (frames, fp),
            "{label} {plane} frame state"
        );
    }
}

#[test]
fn text_overrun_and_unliftable_instructions_trap_only_when_reached() {
    use rax::isa::ebpf::{EbpfDecodeError, MM_BYTECODE_START};
    let le = Endian::Little;
    // Falling off the end of the text: the overrun slot is charged like the
    // reference's ExecutionOverrun, and SMIR lifts the program.
    for version in [SbpfVersion::V0, SbpfVersion::V2, SbpfVersion::V3] {
        let program = [
            encode_insn(0xb7, 0, 0, 0, 1, le),
            encode_insn(0x07, 0, 0, 0, 1, le),
        ]
        .concat();
        let end = MM_BYTECODE_START + 16;
        assert_planes(
            &format!("{version:?} overrun"),
            &sbpf_planes(version, &program, &[]),
            Err(EbpfTrap::CallOutsideTextSegment {
                pc: end,
                target_pc: end,
            }),
            3,
        );
    }

    // V3: an unresolvable call kind (src=2) verifies upstream; it traps only
    // when executed, after consuming its compute unit.
    let skipped = [
        encode_insn(0x05, 0, 0, 1, 0, le),
        encode_insn(0x85, 0, 2, 0, 7, le),
        encode_insn(0x95, 0, 0, 0, 0, le),
    ]
    .concat();
    assert_planes(
        "V3 unexecuted call kind",
        &sbpf_planes(SbpfVersion::V3, &skipped, &[(0, 9)]),
        Ok(9),
        2,
    );
    let executed = [
        encode_insn(0x85, 0, 2, 0, 7, le),
        encode_insn(0x95, 0, 0, 0, 0, le),
    ]
    .concat();
    assert_planes(
        "V3 executed call kind",
        &sbpf_planes(SbpfVersion::V3, &executed, &[]),
        Err(EbpfTrap::DecodeError {
            pc: MM_BYTECODE_START,
            error: EbpfDecodeError::ReservedField { opc: 0x85 },
        }),
        1,
    );

    // RFC 9669: a pseudo-load naming an unbound map on an untaken path does
    // not prevent lifting; reaching it raises the same trap on every plane.
    let config = EbpfConfig::default();
    for taken in [false, true] {
        let program = [
            encode_insn(0x15, 1, 0, 2, 0, le),
            encode_insn(0x18, 0, 1, 0, 5, le),
            encode_insn(0x00, 0, 0, 0, 0, le),
            encode_insn(0x95, 0, 0, 0, 0, le),
        ]
        .concat();
        // jeq r1, 0 skips the pseudo-load; r1 = 1 reaches it.
        let regs = [(0, 3), (1, u64::from(taken))];
        let (exit, count, _) = direct_trap(&program, &config, &regs, 64);
        let expected = if taken {
            EbpfExit::Trap(EbpfTrap::UnsupportedPseudoLoad { pc: 8, src_type: 1 })
        } else {
            EbpfExit::Exit { r0: 3 }
        };
        assert_eq!(exit, expected);
        for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let (reason, smir_count, _) = smir_trap(&program, &config, &regs, 64, opt);
            match expected {
                EbpfExit::Trap(trap) => {
                    assert!(
                        matches!(reason, ExitReason::EbpfTrap(t) if t == trap),
                        "{reason:?}"
                    )
                }
                _ => assert!(matches!(reason, ExitReason::Return { to: 3 }), "{reason:?}"),
            }
            assert_eq!(smir_count, count, "{opt:?} taken={taken}");
        }
    }

    // RFC 9669: a BTF helper call (src=2) without a registered handler exits
    // to the host at its continuation on every plane, in the BTF namespace.
    for taken in [false, true] {
        let program = [
            encode_insn(0x15, 1, 0, 1, 0, le),
            encode_insn(0x85, 0, 2, 0, 9, le),
            encode_insn(0x95, 0, 0, 0, 0, le),
        ]
        .concat();
        let regs = [(0, 3), (1, u64::from(taken))];
        let (exit, count, pc) = direct_trap(&program, &config, &regs, 64);
        let expected = if taken {
            EbpfExit::BtfHelper { id: 9, next_pc: 16 }
        } else {
            EbpfExit::Exit { r0: 3 }
        };
        assert_eq!(exit, expected);
        for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let (reason, smir_count, smir_pc) = smir_trap(&program, &config, &regs, 64, opt);
            if taken {
                assert!(
                    matches!(
                        &reason,
                        ExitReason::Syscall { num, .. } if *num == EbpfHelperKey::Btf(9).smir_key()
                    ),
                    "{reason:?}"
                );
                assert_eq!(smir_pc, pc, "BTF {opt:?}");
            } else {
                assert!(matches!(reason, ExitReason::Return { to: 3 }), "{reason:?}");
            }
            assert_eq!(smir_count, count, "BTF {opt:?} taken={taken}");
        }
    }

    // RFC 9669 malformed instruction and a 16-byte LDDW cut off by the end
    // of readable memory, with and without a text range.
    let malformed = [
        encode_insn(0xb7, 0, 0, 0, 1, le),
        encode_insn(0xff, 0, 0, 0, 0, le),
    ]
    .concat();
    let truncated_lddw = [
        encode_insn(0xb7, 0, 0, 0, 1, le),
        encode_insn(0x18, 0, 0, 0, 1, le),
    ]
    .concat();
    for (program, text_range, expected) in [
        (
            &malformed,
            None,
            EbpfTrap::DecodeError {
                pc: 8,
                error: EbpfDecodeError::InvalidOpcode { opc: 0xff },
            },
        ),
        (
            &truncated_lddw,
            None,
            EbpfTrap::MemoryFault {
                pc: 8,
                fault: EbpfMemFault {
                    addr: 16,
                    size: 8,
                    write: false,
                },
            },
        ),
        (
            &truncated_lddw,
            Some((0, 16)),
            EbpfTrap::DecodeError {
                pc: 8,
                error: EbpfDecodeError::UnexpectedEof {
                    needed: 16,
                    available: 8,
                },
            },
        ),
    ] {
        let config = EbpfConfig {
            text_range,
            ..EbpfConfig::default()
        };
        let (exit, count, _) = direct_trap(program, &config, &[], 16);
        assert_eq!(exit, EbpfExit::Trap(expected), "{text_range:?}");
        assert_eq!(count, 2);
        for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let (reason, smir_count, _) = smir_trap(program, &config, &[], 16, opt);
            assert!(
                matches!(reason, ExitReason::EbpfTrap(t) if t == expected),
                "{text_range:?} {opt:?}: {reason:?}"
            );
            assert_eq!(smir_count, 2, "{text_range:?} {opt:?}");
        }
    }
}

#[test]
fn sbpf_call_entry_follows_the_reference_order_and_stays_precise() {
    use rax::isa::ebpf::MM_BYTECODE_START;
    let le = Endian::Little;
    // V3/V4: a relative call outside the text is unresolved (no frame).
    for version in [SbpfVersion::V3, SbpfVersion::V4] {
        let program = [
            encode_insn(0x85, 0, 1, 0, 5, le),
            encode_insn(0x95, 0, 0, 0, 0, le),
        ]
        .concat();
        let planes = sbpf_planes(version, &program, &[]);
        assert_planes(
            &format!("{version:?} relative call outside text"),
            &planes,
            Err(EbpfTrap::UnsupportedSyscall {
                pc: MM_BYTECODE_START,
                key: 5,
            }),
            1,
        );
        assert_eq!(planes[0].3, 0, "no frame was pushed");
    }

    // CALLX recursion into an out-of-text target once the depth is spent:
    // the depth fault wins (frame push precedes the target check).
    for (version, callx) in [
        (SbpfVersion::V0, encode_insn(0x8d, 0, 0, 0, 1, le)),
        (SbpfVersion::V2, encode_insn(0x8d, 0, 1, 0, 0, le)),
        (SbpfVersion::V3, encode_insn(0x8d, 1, 0, 0, 0, le)),
    ] {
        let start = MM_BYTECODE_START;
        // r1 = text start (recursive CALLX) for three nested calls, then an
        // out-of-text value: `add64 r2, 1; jlt r2, 4, +1; r1 = 0x40; callx r1`.
        // With a maximum depth of 4 frames the fourth CALLX exceeds it.
        let program = [
            encode_insn(0x07, 2, 0, 0, 1, le),
            encode_insn(0xa5, 2, 0, 1, 4, le),
            encode_insn(0xb7, 1, 0, 0, 0x40, le),
            callx,
            encode_insn(0x95, 0, 0, 0, 0, le),
        ]
        .concat();
        let planes = sbpf_planes(version, &program, &[(1, start), (2, 0)]);
        let depth_pc = start + 24;
        assert_planes(
            &format!("{version:?} depth before target"),
            &planes,
            Err(EbpfTrap::CallDepthExceeded { pc: depth_pc }),
            // Three levels of (add, jlt, callx), then add, jlt, mov, callx.
            13,
        );

        // A single invalid target below the depth limit traps without
        // leaving the pushed frame or a bumped frame pointer behind.
        let program = [callx, encode_insn(0x95, 0, 0, 0, 0, le)].concat();
        let planes = sbpf_planes(version, &program, &[(1, 0x40)]);
        assert_planes(
            &format!("{version:?} invalid target"),
            &planes,
            Err(EbpfTrap::CallOutsideTextSegment {
                pc: start,
                target_pc: 0x40,
            }),
            1,
        );
        let initial_fp = EbpfCpu::new(EbpfConfig {
            max_call_depth: 4,
            ..EbpfConfig::solana(version)
        })
        .regs
        .r[10];
        assert_eq!((planes[0].3, planes[0].4), (0, initial_fp));
    }
}
