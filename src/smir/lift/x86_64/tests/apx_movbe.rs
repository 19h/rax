//! Strict lifting, interpretation, optimization, and native coverage for APX MOVBE.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::x86_64::X86_64Lowerer;

const INITIAL_DESTINATION: u64 = 0xA1B2_C3D4_E5F6_7788;
const INITIAL_FLAGS: u64 = 0xCD7;

fn p1(width: OpWidth) -> u8 {
    match width {
        OpWidth::W16 => 0x7D,
        OpWidth::W32 => 0x7C,
        OpWidth::W64 => 0xFC,
        _ => unreachable!(),
    }
}

fn movbe_encoding(width: OpWidth, opcode: u8, memory: bool) -> Vec<u8> {
    let (p0, modrm) = match (opcode, memory) {
        (0x60, false) => (0x74, 0xC3), // r8 <- rbx
        (0x60, true) => (0x74, 0x03),  // r8 <- [rbx]
        (0x61, false) => (0xD4, 0xC0), // r8 <- rax
        (0x61, true) => (0x74, 0x03),  // [rbx] <- r8
        _ => unreachable!(),
    };
    vec![0x62, p0, p1(width), 0x08, opcode, modrm]
}

fn swap(value: u64, width: OpWidth) -> u64 {
    match width {
        OpWidth::W16 => u64::from((value as u16).swap_bytes()),
        OpWidth::W32 => u64::from((value as u32).swap_bytes()),
        OpWidth::W64 => value.swap_bytes(),
        _ => unreachable!(),
    }
}

fn merge_gpr(old: u64, value: u64, width: OpWidth) -> u64 {
    match width {
        OpWidth::W16 => (old & !0xFFFF) | (value & 0xFFFF),
        OpWidth::W32 => value as u32 as u64,
        OpWidth::W64 => value,
        _ => unreachable!(),
    }
}

fn assert_guard(result: &LiftResult) {
    assert!(matches!(
        result.ops.first(),
        Some(SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::X86RequireApx,
            x86_hint: None,
        })
    ));
}

#[test]
fn apx_movbe_strictly_lifts_both_directions_widths_and_source_classes() {
    for opcode in [0x60, 0x61] {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            for memory in [false, true] {
                let bytes = movbe_encoding(width, opcode, memory);
                let result = lift_single(&bytes).expect("strictly lift APX MOVBE");
                assert_eq!(result.bytes_consumed, bytes.len());
                assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
                assert_guard(&result);
                for (index, op) in result.ops.iter().enumerate() {
                    assert_eq!(op.id, OpId(index as u16));
                    assert_eq!(op.guest_pc, 0x1000);
                }

                let bswap = result
                    .ops
                    .iter()
                    .find(|op| matches!(op.kind, OpKind::Bswap { .. }))
                    .expect("MOVBE must contain one Bswap");
                let OpKind::Bswap {
                    dst,
                    src,
                    width: got_width,
                } = bswap.kind
                else {
                    unreachable!()
                };
                assert_eq!(got_width, width);

                match (opcode, memory) {
                    (0x60, false) => {
                        assert_eq!(dst, x86_gpr(8));
                        assert_eq!(src, x86_gpr(3));
                        assert_eq!(result.ops.len(), 2);
                    }
                    (0x60, true) => {
                        let load = result
                            .ops
                            .iter()
                            .find(|op| matches!(op.kind, OpKind::Load { .. }))
                            .expect("MOVBE load form must read memory");
                        assert!(matches!(
                            load.kind,
                            OpKind::Load {
                                dst: loaded,
                                addr: Address::Direct(base),
                                width: got_width,
                                sign: SignExtend::Zero,
                            } if loaded == src
                                && base == x86_gpr(3)
                                && got_width == width.to_mem_width()
                        ));
                        assert_eq!(dst, x86_gpr(8));
                        assert_eq!(result.ops.len(), 3);
                    }
                    (0x61, false) => {
                        assert_eq!(dst, x86_gpr(8));
                        assert_eq!(src, x86_gpr(0));
                        assert_eq!(result.ops.len(), 2);
                    }
                    (0x61, true) => {
                        assert!(matches!(dst, VReg::Virtual(_)));
                        assert_eq!(src, x86_gpr(8));
                        let store = result
                            .ops
                            .iter()
                            .find(|op| matches!(op.kind, OpKind::Store { .. }))
                            .expect("MOVBE store form must write memory");
                        assert!(matches!(
                            store.kind,
                            OpKind::Store {
                                src: stored,
                                addr: Address::Direct(base),
                                width: got_width,
                            } if stored == dst
                                && base == x86_gpr(3)
                                && got_width == width.to_mem_width()
                        ));
                        assert_eq!(result.ops.len(), 3);
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    // SCALABLE gives W precedence over pp=66.
    let w1_pp66 = lift_single(&[0x62, 0x74, 0xFD, 0x08, 0x60, 0xC3]).unwrap();
    assert!(matches!(
        w1_pp66.ops.last(),
        Some(SmirOp {
            kind: OpKind::Bswap {
                width: OpWidth::W64,
                ..
            },
            ..
        })
    ));
}

#[test]
fn apx_movbe_lifts_egpr_register_and_memory_index_extensions() {
    for opcode in [0x60, 0x61] {
        let result = lift_single(&[0x62, 0x6C, 0xFC, 0x08, opcode, 0xC0]).unwrap();
        assert_guard(&result);
        assert!(matches!(
            result.ops.last(),
            Some(SmirOp {
                kind: OpKind::Bswap {
                    dst,
                    src,
                    width: OpWidth::W64,
                },
                ..
            }) if *dst == if opcode == 0x60 { x86_gpr(24) } else { x86_gpr(16) }
                && *src == if opcode == 0x60 { x86_gpr(16) } else { x86_gpr(24) }
        ));
    }

    // LLVM 20: `movbeq (%r16,%r17,2), %r18`.
    let load = lift_single(&[0x62, 0xEC, 0xF8, 0x08, 0x60, 0x14, 0x48]).unwrap();
    assert_guard(&load);
    assert!(matches!(
        load.ops.get(1),
        Some(SmirOp {
            kind: OpKind::Load {
                addr: Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 2,
                    disp: 0,
                    ..
                },
                width: MemWidth::B8,
                ..
            },
            ..
        }) if *base == x86_gpr(16) && *index == x86_gpr(17)
    ));
    assert!(matches!(
        load.ops.last(),
        Some(SmirOp {
            kind: OpKind::Bswap {
                dst,
                width: OpWidth::W64,
                ..
            },
            ..
        }) if *dst == x86_gpr(18)
    ));
}

fn assert_movbe_ud(bytes: &[u8], name: &str) {
    let result = lift_single(bytes)
        .unwrap_or_else(|error| panic!("{name} must strictly lift to #UD: {error:?}"));
    assert_invalid_opcode_trap(&result, 6);
}

#[test]
fn apx_movbe_reserved_payloads_strictly_trap_at_modrm_frontier() {
    for (bytes, name) in [
        (&[0x62, 0x74, 0x7E, 0x08, 0x60, 0xC3][..], "F3 pp"),
        (&[0x62, 0x74, 0x7F, 0x08, 0x61, 0xC3][..], "F2 pp"),
        (&[0x62, 0x74, 0x7C, 0x18, 0x60, 0xC3][..], "ND"),
        (&[0x62, 0x74, 0x7C, 0x0C, 0x61, 0xC3][..], "NF"),
        (&[0x62, 0x74, 0x7C, 0x88, 0x60, 0xC3][..], "z"),
        (&[0x62, 0x74, 0x7C, 0x28, 0x61, 0xC3][..], "LL"),
        (&[0x62, 0x74, 0x7C, 0x09, 0x60, 0xC3][..], "payload bit 0"),
        (&[0x62, 0x74, 0x74, 0x08, 0x61, 0xC3][..], "V3:0"),
        (&[0x62, 0x74, 0x7C, 0x00, 0x60, 0xC3][..], "V4"),
        (&[0x62, 0x74, 0x78, 0x08, 0x61, 0xC3][..], "register U"),
    ] {
        assert_movbe_ud(bytes, name);
    }

    // Reserved prefix fields establish #UD after ModR/M without an apparent
    // SIB/displacement tail.
    assert_movbe_ud(&[0x62, 0x74, 0x7C, 0x18, 0x60, 0x84], "ND memory tail");

    for bytes in [
        &[0xF0, 0x62, 0x74, 0x7C, 0x08, 0x60, 0xC3][..],
        &[0x66, 0x62, 0x74, 0x7C, 0x08, 0x61, 0xC3],
        &[0x48, 0x62, 0x74, 0x7C, 0x08, 0x60, 0xC3],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn apx_movbe_incomplete_lengths_are_absolute() {
    for opcode in [0x60, 0x61] {
        for p2 in [0x08, 0x0C, 0x18] {
            assert!(matches!(
                lift_single(&[0x62, 0x74, 0x7C, p2, opcode]),
                Err(LiftError::Incomplete {
                    have: 5,
                    need: 6,
                    ..
                })
            ));
        }
    }
    assert!(matches!(
        lift_single(&[0x62, 0x74, 0x7C, 0x08, 0x60, 0x84]),
        Err(LiftError::Incomplete {
            have: 6,
            need: 7,
            ..
        })
    ));
    assert!(matches!(
        lift_single(&[0x62, 0x74, 0x7C, 0x08, 0x61, 0x84, 0x03]),
        Err(LiftError::Incomplete {
            have: 7,
            need: 11,
            ..
        })
    ));
}

fn movbe_function(bytes: &[u8]) -> SmirFunction {
    let result = lift_single(bytes).expect("lift guarded APX MOVBE");
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
fn apx_movbe_x86_jit_gate_admits_register_and_helper_backed_memory_shapes() {
    use crate::smir::lower::runtime::{is_native_clobber_safe, is_native_clobber_safe_excluding};

    for opcode in [0x60, 0x61] {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            for memory in [false, true] {
                let bytes = movbe_encoding(width, opcode, memory);
                let mut function = movbe_function(&bytes);
                let entry = function.entry;
                function
                    .get_block_mut(entry)
                    .unwrap()
                    .set_terminator(Terminator::Return { values: vec![] });
                let label = format!("opcode={opcode:#04x} width={width:?} memory={memory}");
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

#[test]
fn apx_movbe_guard_and_register_semantics_survive_o2() {
    const SOURCE: u64 = 0x0123_4567_89AB_CDEF;

    for opcode in [0x60, 0x61] {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let original = movbe_function(&movbe_encoding(width, opcode, false));
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
                    context.write_vreg(x86_gpr(8), INITIAL_DESTINATION);
                    context.write_vreg(
                        if opcode == 0x60 {
                            x86_gpr(3)
                        } else {
                            x86_gpr(0)
                        },
                        SOURCE,
                    );
                    context.flags.materialized = MaterializedFlags::from_rflags(INITIAL_FLAGS);
                    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                        unreachable!()
                    };
                    x86.apx_enabled = enabled;

                    let execution = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut FlatMemory::new(1),
                        function.entry_block().unwrap(),
                    );
                    if enabled {
                        assert!(matches!(execution, BlockResult::Exit(ExitReason::Halt)));
                        assert_eq!(
                            context.read_vreg(x86_gpr(8)),
                            merge_gpr(INITIAL_DESTINATION, swap(SOURCE, width), width)
                        );
                    } else {
                        assert!(matches!(
                            execution,
                            BlockResult::Exit(ExitReason::Undefined {
                                addr: 0x1000,
                                opcode: 0,
                            })
                        ));
                        assert_eq!(context.read_vreg(x86_gpr(8)), INITIAL_DESTINATION);
                    }
                    assert_eq!(context.flags.materialized.to_rflags(), INITIAL_FLAGS);
                }
            }
        }
    }
}

#[test]
fn apx_movbe_memory_faults_and_disabled_guard_do_not_commit() {
    const SOURCE: u64 = 0x0123_4567_89AB_CDEF;

    for opcode in [0x60, 0x61] {
        let function = movbe_function(&movbe_encoding(OpWidth::W64, opcode, true));
        for enabled in [false, true] {
            let mut context = SmirContext::new_x86_64();
            context.write_vreg(x86_gpr(3), 0x200);
            context.write_vreg(
                x86_gpr(8),
                if opcode == 0x60 {
                    INITIAL_DESTINATION
                } else {
                    SOURCE
                },
            );
            context.flags.materialized = MaterializedFlags::from_rflags(INITIAL_FLAGS);
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.apx_enabled = enabled;
            let execution = SmirInterpreter::new().execute_block(
                &mut context,
                &mut FlatMemory::new(0x40),
                function.entry_block().unwrap(),
            );

            if enabled {
                assert!(matches!(
                    execution,
                    BlockResult::Exit(ExitReason::MemoryFault {
                        addr: 0x200,
                        write,
                    }) if write == (opcode == 0x61)
                ));
            } else {
                assert!(matches!(
                    execution,
                    BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
                ));
            }
            assert_eq!(
                context.read_vreg(x86_gpr(8)),
                if opcode == 0x60 {
                    INITIAL_DESTINATION
                } else {
                    SOURCE
                }
            );
            assert_eq!(context.flags.materialized.to_rflags(), INITIAL_FLAGS);
        }
    }
}

#[test]
fn apx_movbe_interpreter_load_and_store_are_exact() {
    const SOURCE: u64 = 0x0123_4567_89AB_CDEF;
    const ADDRESS: u64 = 0x20;

    for opcode in [0x60, 0x61] {
        let function = movbe_function(&movbe_encoding(OpWidth::W64, opcode, true));
        let mut context = SmirContext::new_x86_64();
        context.write_vreg(x86_gpr(3), ADDRESS);
        context.write_vreg(
            x86_gpr(8),
            if opcode == 0x60 {
                INITIAL_DESTINATION
            } else {
                SOURCE
            },
        );
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.apx_enabled = true;
        let mut memory = FlatMemory::new(0x100);
        if opcode == 0x60 {
            memory.write(ADDRESS, &SOURCE.to_le_bytes()).unwrap();
        }
        let execution = SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            function.entry_block().unwrap(),
        );
        assert!(matches!(execution, BlockResult::Exit(ExitReason::Halt)));
        if opcode == 0x60 {
            assert_eq!(context.read_vreg(x86_gpr(8)), SOURCE.swap_bytes());
        } else {
            let mut observed = [0_u8; 8];
            memory.read(ADDRESS, &mut observed).unwrap();
            assert_eq!(u64::from_le_bytes(observed), SOURCE.swap_bytes());
            assert_eq!(context.read_vreg(x86_gpr(8)), SOURCE);
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn apx_movbe_native_guard_is_dynamic_precise_and_noncommitting() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const SOURCE: u64 = 0x0123_4567_89AB_CDEF;
    const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;

    for opcode in [0x60, 0x61] {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let mut function = movbe_function(&movbe_encoding(width, opcode, false));
            let entry = function.entry;
            function
                .get_block_mut(entry)
                .unwrap()
                .set_terminator(Terminator::Return { values: vec![] });
            let mut lowerer = X86_64Lowerer::new();
            lowerer.set_jit_fault_deopt_guards(true);
            let lowered = lowerer.lower_function(&function).expect("lower APX MOVBE");
            let executable = ExecMem::new(&lowerer.finalize().unwrap()).unwrap();

            for enabled in [false, true] {
                let mut registers = GuestRegs::default();
                registers.gpr[8] = INITIAL_DESTINATION;
                registers.gpr[if opcode == 0x60 { 3 } else { 0 }] = SOURCE;
                registers.rflags = INITIAL_FLAGS;
                registers.apx_enabled = u64::from(enabled);
                registers.exit_pc = SENTINEL_PC;
                let initial = registers;

                executable.run(lowered.entry_offset, &mut registers);

                if enabled {
                    assert_eq!(
                        registers.gpr[8],
                        merge_gpr(INITIAL_DESTINATION, swap(SOURCE, width), width),
                        "opcode={opcode:#04x} width={width:?}"
                    );
                    assert_eq!(registers.exit_pc, SENTINEL_PC);
                } else {
                    assert_eq!(registers.gpr, initial.gpr);
                    assert_eq!(registers.exit_pc, 0x1000);
                }
                assert_eq!(
                    registers.rflags & INITIAL_FLAGS,
                    initial.rflags & INITIAL_FLAGS
                );
            }
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn apx_movbe_native_memory_guard_helper_faults_and_o2_are_precise() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    #[repr(C)]
    struct LoadResult {
        value: u64,
        ok: u64,
    }

    #[derive(Default)]
    struct MemoryContext {
        value: u64,
        ok: u64,
        calls: u64,
        last_addr: u64,
        last_size: u64,
        stored: u64,
    }

    extern "C" fn load(
        context: *mut MemoryContext,
        addr: u64,
        size: u64,
        _signed: u64,
    ) -> LoadResult {
        let context = unsafe { &mut *context };
        context.calls += 1;
        context.last_addr = addr;
        context.last_size = size;
        LoadResult {
            value: context.value,
            ok: context.ok,
        }
    }

    extern "C" fn store(context: *mut MemoryContext, addr: u64, value: u64, size: u64) -> u64 {
        let context = unsafe { &mut *context };
        context.calls += 1;
        context.last_addr = addr;
        context.last_size = size;
        context.stored = value;
        context.ok
    }

    const SOURCE: u64 = 0x0123_4567_89AB_CDEF;
    const ADDRESS: u64 = 0x4000;
    const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;

    for opcode in [0x60, 0x61] {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            for optimize in [false, true] {
                let mut function = movbe_function(&movbe_encoding(width, opcode, true));
                let entry = function.entry;
                function
                    .get_block_mut(entry)
                    .unwrap()
                    .set_terminator(Terminator::Return { values: vec![] });
                if optimize {
                    crate::smir::optimize::optimize_function(
                        &mut function,
                        crate::smir::optimize::OptLevel::O2,
                    );
                }
                assert!(
                    crate::smir::lower::runtime::is_native_clobber_safe_excluding(
                        &function,
                        &std::collections::HashMap::new(),
                        true,
                    )
                );

                let mut lowerer = X86_64Lowerer::new();
                lowerer.set_mem_helpers(true);
                lowerer.set_jit_fault_deopt_guards(true);
                let lowered = lowerer
                    .lower_function(&function)
                    .expect("lower APX MOVBE memory");
                let executable = ExecMem::new(&lowerer.finalize().unwrap()).unwrap();

                for (enabled, ok) in [(false, 1), (true, 0), (true, 1)] {
                    let mut context = MemoryContext {
                        value: SOURCE,
                        ok,
                        ..MemoryContext::default()
                    };
                    let mut registers = GuestRegs::default();
                    registers.gpr[3] = ADDRESS;
                    registers.gpr[8] = if opcode == 0x60 {
                        INITIAL_DESTINATION
                    } else {
                        SOURCE
                    };
                    registers.rflags = INITIAL_FLAGS;
                    registers.apx_enabled = u64::from(enabled);
                    registers.exit_pc = SENTINEL_PC;
                    registers.ctx = (&mut context as *mut MemoryContext) as u64;
                    registers.load_fn = load as usize as u64;
                    registers.store_fn = store as usize as u64;
                    let initial_gprs = registers.gpr;

                    executable.run(lowered.entry_offset, &mut registers);

                    let committed = enabled && ok != 0;
                    let label = format!(
                        "opcode={opcode:#04x} width={width:?} O2={optimize} APX={enabled} ok={ok}"
                    );
                    assert_eq!(context.calls, u64::from(enabled), "{label}");
                    if enabled {
                        assert_eq!(
                            (context.last_addr, context.last_size),
                            (ADDRESS, width.bytes() as u64),
                            "{label}"
                        );
                    }
                    if committed && opcode == 0x60 {
                        assert_eq!(
                            registers.gpr[8],
                            merge_gpr(INITIAL_DESTINATION, swap(SOURCE, width), width),
                            "{label}"
                        );
                    } else {
                        assert_eq!(registers.gpr, initial_gprs, "{label}");
                    }
                    if enabled && opcode == 0x61 {
                        let mask = match width {
                            OpWidth::W16 => 0xFFFF,
                            OpWidth::W32 => 0xFFFF_FFFF,
                            OpWidth::W64 => u64::MAX,
                            _ => unreachable!(),
                        };
                        assert_eq!(context.stored & mask, swap(SOURCE, width) & mask, "{label}");
                    }
                    assert_eq!(registers.rflags & INITIAL_FLAGS, INITIAL_FLAGS, "{label}");
                    assert_eq!(
                        registers.exit_pc,
                        if committed { SENTINEL_PC } else { 0x1000 },
                        "{label}"
                    );
                }
            }
        }
    }
}
