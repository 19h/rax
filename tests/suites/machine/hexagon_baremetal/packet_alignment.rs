//! V68 section 5.1.3: native-size alignment precedes data or register effects.
use rax::config::{Endianness, HexagonIsa};
use rax::error::{Error, MemoryAccessKind};
use rax::isa::hexagon::HexagonVcpu;
use rax::vm::vcpu::{CpuState, HexagonRegisters, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const LOADS: [(u32, u32); 11] = [
    (0x9141c003, 2),
    (0x9161c003, 2),
    (0x9181c003, 4),
    (0x91c1c002, 8),
    (0x9b81c023, 4),
    (0x9b41c023, 2), // halfword immediate post-increment
    (0x9d41c003, 2), // modifier post-increment
    (0x9941c023, 2), // circular post-increment
    (0x9061c003, 2), // two byte-to-halfword lanes
    (0x90a1c002, 4), // four byte-to-halfword lanes
    (0x9201c003, 4), // locked word load
];
const STORES: [(u32, u32); 3] = [(0xa141c200, 2), (0xa181c200, 4), (0xa1c1c200, 8)];
fn assert_alignment(error: Error, pc: u64, address: u64, alignment: u32, access: MemoryAccessKind) {
    assert!(
        matches!(error, Error::AlignmentFault { pc:p, address:a, alignment:n, access:k }
        if p == pc && a == address && n == alignment && k == access),
        "{error:?}"
    );
}
fn machine(
    word: u32,
    address: u32,
    pc: u32,
    endian: Endianness,
) -> (HexagonVcpu, Arc<GuestMemoryMmap>, HexagonRegisters) {
    let memory = Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x4000)]).unwrap());
    let code = match endian {
        Endianness::Little => word.to_le_bytes(),
        Endianness::Big => word.to_be_bytes(),
    };
    memory.write_slice(&code, GuestAddress(pc as u64)).unwrap();
    memory
        .write_slice(&[0xa5; 32], GuestAddress(0x2000))
        .unwrap();
    let mut regs = HexagonRegisters::default();
    regs.set_pc(pc);
    regs.r[1] = address;
    regs.r[2] = 0x11223344;
    regs.r[3] = 0x55667788;
    let mut cpu = HexagonVcpu::new(0, memory.clone(), HexagonIsa::V68, endian);
    cpu.set_state(&CpuState::hexagon(regs.clone())).unwrap();
    (cpu, memory, regs)
}
fn regs(cpu: &HexagonVcpu) -> HexagonRegisters {
    let CpuState::Hexagon(state) = cpu.get_state().unwrap() else {
        unreachable!()
    };
    state.regs
}
#[test]
fn unaligned_scalar_loads_fault_without_destination_or_postincrement_commit() {
    for endian in [Endianness::Little, Endianness::Big] {
        for (word, width) in LOADS {
            for offset in 1..width {
                let (mut cpu, _, before) = machine(word, 0x2000 + offset, 0x1000, endian);
                assert_alignment(
                    cpu.step_insn().unwrap_err(),
                    0x1000,
                    (0x2000 + offset) as u64,
                    width,
                    MemoryAccessKind::Read,
                );
                assert_eq!(regs(&cpu).r, before.r);
                assert_eq!(cpu.current_pc(), 0x1000);
            }
        }
    }
}
#[test]
fn unaligned_scalar_stores_fault_without_any_memory_write() {
    for endian in [Endianness::Little, Endianness::Big] {
        for (word, width) in STORES {
            for offset in 1..width {
                let (mut cpu, memory, before) = machine(word, 0x2000 + offset, 0x1000, endian);
                assert_alignment(
                    cpu.step_insn().unwrap_err(),
                    0x1000,
                    (0x2000 + offset) as u64,
                    width,
                    MemoryAccessKind::Write,
                );
                let mut after = [0; 32];
                memory.read_slice(&mut after, GuestAddress(0x2000)).unwrap();
                assert_eq!(after, [0xa5; 32]);
                assert_eq!(regs(&cpu).r, before.r);
                assert_eq!(cpu.current_pc(), 0x1000);
            }
        }
    }
}
#[test]
fn aligned_scalar_memory_instructions_remain_executable() {
    for endian in [Endianness::Little, Endianness::Big] {
        for (word, _) in LOADS.into_iter().chain(STORES) {
            let (mut cpu, _, _) = machine(word, 0x2000, 0x1000, endian);
            assert!(cpu.step_insn().unwrap().is_none());
            assert_eq!(cpu.current_pc(), 0x1004);
        }
    }
}
#[test]
fn false_predicates_cancel_unaligned_memory_accesses() {
    for endian in [Endianness::Little, Endianness::Big] {
        for word in [0x4181c003, 0x4081c200] {
            let (mut cpu, memory, before) = machine(word, 0x2001, 0x1000, endian);
            assert!(cpu.step_insn().unwrap().is_none());
            assert_eq!(regs(&cpu).r, before.r);
            let mut after = [0; 32];
            memory.read_slice(&mut after, GuestAddress(0x2000)).unwrap();
            assert_eq!(after, [0xa5; 32]);
        }
    }
}
#[test]
fn unaligned_mmio_is_rejected_before_emitting_an_external_request() {
    for word in [0x9181c003, 0xa181c200] {
        let (mut cpu, _, before) = machine(word, 0xf0000001, 0x1000, Endianness::Little);
        let access = if word == 0x9181c003 {
            MemoryAccessKind::Read
        } else {
            MemoryAccessKind::Write
        };
        assert_alignment(cpu.step_insn().unwrap_err(), 0x1000, 0xf0000001, 4, access);
        assert_eq!(regs(&cpu).r, before.r);
    }
}
#[test]
fn unaligned_instruction_fetch_faults_before_decoding_valid_bytes() {
    let (mut cpu, _, before) = machine(0x9181c003, 0x2000, 0x1001, Endianness::Little);
    assert_alignment(
        cpu.step_insn().unwrap_err(),
        0x1001,
        0x1001,
        4,
        MemoryAccessKind::Fetch,
    );
    assert_eq!(regs(&cpu).r, before.r);
    assert_eq!(cpu.current_pc(), 0x1001);
}

#[path = "../../../support/hexagon.rs"]
mod lifter_stack;

fn lifted(
    words: &[u32],
    registers: &HexagonRegisters,
    level: rax::smir::optimize::OptLevel,
) -> (
    rax::smir::SmirContext,
    rax::smir::FlatMemory,
    rax::smir::BlockResult,
) {
    use rax::smir::ir::memory::SmirMemory;
    use rax::smir::ir::types::{ArchReg, FunctionId, HexagonReg, SourceArch};
    use rax::smir::ir::{FunctionBuilder, Terminator, TrapKind};
    use rax::smir::{HexagonLifter, LiftContext, SmirLifter};
    lifter_stack::run_on_large_lifter_stack("Hexagon alignment contract", || {
        let mut lifter = HexagonLifter::default_isa();
        let mut context = LiftContext::new(SourceArch::Hexagon);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        for (index, word) in words.iter().enumerate() {
            let lifted = lifter
                .lift_insn(0x1000 + index as u64 * 4, &word.to_le_bytes(), &mut context)
                .unwrap();
            for op in lifted.ops {
                builder.push_op(op.guest_pc, op.kind);
            }
        }
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let mut function = builder.finish();
        rax::smir::optimize::optimize_function(&mut function, level);
        let mut state = rax::smir::SmirContext::new_hexagon();
        for n in 0..32 {
            state.write_arch_reg(
                ArchReg::Hexagon(HexagonReg::R(n)),
                registers.r[n as usize] as u64,
            );
        }
        for n in 0..4 {
            state.write_arch_reg(
                ArchReg::Hexagon(HexagonReg::P(n)),
                registers.p[n as usize] as u64,
            );
        }
        let mut memory = rax::smir::FlatMemory::new(0x4000);
        memory.write(0x2000, &[0xa5; 32]).unwrap();
        let result = rax::smir::SmirInterpreter::new().execute_block(
            &mut state,
            &mut memory,
            function.entry_block().unwrap(),
        );
        (state, memory, result)
    })
}

#[test]
fn lifted_scalar_alignment_faults_preserve_registers_and_memory_at_every_opt_level() {
    use rax::error::MemoryAccessKind;
    use rax::smir::ir::memory::SmirMemory;
    use rax::smir::ir::types::{ArchReg, HexagonReg};
    use rax::smir::optimize::OptLevel;
    use rax::smir::{BlockResult, ExitReason};
    for (cases, access) in [
        (&LOADS[..], MemoryAccessKind::Read),
        (&STORES[..], MemoryAccessKind::Write),
    ] {
        for &(word, width) in cases {
            for offset in 1..width {
                let address = 0x2000 + offset;
                let (_, _, before) = machine(word, address, 0x1000, Endianness::Little);
                for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                    let (state, mut memory, result) = lifted(&[word], &before, level);
                    assert!(
                        matches!(result, BlockResult::Exit(ExitReason::AlignmentFault {
                        pc:0x1000, address:a, alignment:n, access:k
                    }) if a==address as u64 && n==width && k==access),
                        "{word:x} {level:?}: {result:?}"
                    );
                    assert_eq!(state.pc, 0x1000);
                    for n in 0..32 {
                        assert_eq!(
                            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(n))),
                            before.r[n as usize] as u64
                        );
                    }
                    let mut bytes = [0; 32];
                    memory.read(0x2000, &mut bytes).unwrap();
                    assert_eq!(bytes, [0xa5; 32]);
                }
            }
        }
    }
}

#[test]
fn lifted_false_predicates_cancel_misaligned_loads_and_stores() {
    use rax::smir::ir::memory::SmirMemory;
    use rax::smir::ir::types::{ArchReg, HexagonReg};
    use rax::smir::optimize::OptLevel;
    use rax::smir::{BlockResult, ExitReason};
    for word in [0x4181c003, 0x4081c200] {
        let (_, _, before) = machine(word, 0x2001, 0x1000, Endianness::Little);
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let (state, mut memory, result) = lifted(&[word], &before, level);
            assert!(
                matches!(result, BlockResult::Exit(ExitReason::Halt)),
                "{result:?}"
            );
            for n in 0..32 {
                assert_eq!(
                    state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(n))),
                    before.r[n as usize] as u64
                );
            }
            let mut bytes = [0; 32];
            memory.read(0x2000, &mut bytes).unwrap();
            assert_eq!(bytes, [0xa5; 32]);
        }
    }
}

#[test]
fn true_predicates_enforce_alignment_in_direct_and_lifted_execution() {
    use rax::error::{Error, MemoryAccessKind};
    use rax::smir::optimize::OptLevel;
    use rax::smir::{BlockResult, ExitReason};
    for (word, access) in [
        (0x4181c003, MemoryAccessKind::Read),
        (0x4081c200, MemoryAccessKind::Write),
    ] {
        let (mut cpu, _, mut before) = machine(word, 0x2001, 0x1000, Endianness::Little);
        before.p[0] = 0xff;
        cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
        assert!(
            matches!(cpu.step_insn(),Err(Error::AlignmentFault {pc:0x1000,address:0x2001,alignment:4,access:k}) if k==access)
        );
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let (_, _, result) = lifted(&[word], &before, level);
            assert!(
                matches!(result,BlockResult::Exit(ExitReason::AlignmentFault {pc:0x1000,address:0x2001,alignment:4,access:k}) if k==access)
            );
        }
    }
}

#[test]
fn a_later_alignment_fault_reports_the_packet_start() {
    use rax::error::{Error, MemoryAccessKind};
    use rax::smir::optimize::OptLevel;
    use rax::smir::{BlockResult, ExitReason};
    let (mut cpu, memory, before) = machine(0xf2014200, 0x2001, 0x1000, Endianness::Little);
    memory
        .write_obj(0x9181c003u32.to_le(), GuestAddress(0x1004))
        .unwrap();
    assert!(matches!(
        cpu.step_insn(),
        Err(Error::AlignmentFault {
            pc: 0x1000,
            address: 0x2001,
            alignment: 4,
            access: MemoryAccessKind::Read
        })
    ));
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let (_, _, result) = lifted(&[0xf2014200, 0x9181c003], &before, level);
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::AlignmentFault {
                pc: 0x1000,
                address: 0x2001,
                alignment: 4,
                access: MemoryAccessKind::Read
            })
        ));
    }
}

#[test]
fn the_lifter_rejects_unaligned_instruction_addresses() {
    use rax::smir::ir::memory::MemoryError;
    use rax::smir::ir::types::SourceArch;
    use rax::smir::{HexagonLifter, LiftContext, LiftError, SmirLifter};
    for address in [0x1001, 0x1002, 0x1003] {
        let error = HexagonLifter::default_isa()
            .lift_insn(
                address,
                &0x9181c003u32.to_le_bytes(),
                &mut LiftContext::new(SourceArch::Hexagon),
            )
            .unwrap_err();
        assert!(
            matches!(error,LiftError::MemoryError {addr,error:MemoryError::Alignment{addr:a,required:4}} if addr==address && a==address)
        );
    }
}

#[test]
fn frame_bit_reversed_and_rmw_accesses_check_native_alignment_before_effects() {
    use rax::smir::ir::types::{ArchReg, HexagonReg};
    use rax::smir::optimize::OptLevel;
    use rax::smir::{BlockResult, ExitReason};
    // Fixed LLVM 23 V68 encodings. A frame accesses one native doubleword even
    // when the lifter implements its two halves as separate word operations.
    for (word, base, value, address, width, access) in [
        (
            0xa09dc002_u32,
            29,
            0x2009,
            0x2001_u64,
            8,
            MemoryAccessKind::Write,
        ),
        (0x901ec01e, 30, 0x2001, 0x2001, 8, MemoryAccessKind::Read),
        (0x9f41c003, 1, 0x8004, 0x2001, 2, MemoryAccessKind::Read),
        (0x3f41c001, 1, 0x2001, 0x2001, 4, MemoryAccessKind::Write),
    ] {
        for endian in [Endianness::Little, Endianness::Big] {
            let (mut cpu, memory, mut before) = machine(word, 0x2000, 0x1000, endian);
            before.r[base] = value;
            cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
            assert_alignment(cpu.step_insn().unwrap_err(), 0x1000, address, width, access);
            assert_eq!(regs(&cpu).r, before.r);
            let mut bytes = [0; 32];
            memory.read_slice(&mut bytes, GuestAddress(0x2000)).unwrap();
            assert_eq!(bytes, [0xa5; 32]);
            if endian == Endianness::Little {
                for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                    let (state, _, result) = lifted(&[word], &before, level);
                    assert!(
                        matches!(result, BlockResult::Exit(ExitReason::AlignmentFault {
                        pc:0x1000, address:a, alignment:n, access:k
                    }) if a==address && n==width && k==access),
                        "{word:x} {level:?}: {result:?}"
                    );
                    for n in 0..32 {
                        assert_eq!(
                            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(n))),
                            before.r[n as usize] as u64
                        );
                    }
                    assert_eq!(state.pc, 0x1000);
                    assert_eq!(
                        state.read_arch_reg(ArchReg::Hexagon(HexagonReg::Pc)),
                        0x1000
                    );
                }
            }
        }
    }
}

#[test]
fn extended_offsets_keep_the_packet_fault_pc_and_exact_address() {
    use rax::smir::optimize::OptLevel;
    use rax::smir::{BlockResult, ExitReason};
    for (word, access) in [
        (0x9181c003_u32, MemoryAccessKind::Read),
        (0xa181c200, MemoryAccessKind::Write),
    ] {
        let words = [0x00004000, word];
        for endian in [Endianness::Little, Endianness::Big] {
            let (mut cpu, memory, before) = machine(words[0], 0x2001, 0x1000, endian);
            let bytes = match endian {
                Endianness::Little => word.to_le_bytes(),
                Endianness::Big => word.to_be_bytes(),
            };
            memory.write_slice(&bytes, GuestAddress(0x1004)).unwrap();
            assert_alignment(cpu.step_insn().unwrap_err(), 0x1000, 0x2001, 4, access);
            assert_eq!(regs(&cpu).r, before.r);
            if endian == Endianness::Little {
                for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                    let (_, _, result) = lifted(&words, &before, level);
                    assert!(
                        matches!(result, BlockResult::Exit(ExitReason::AlignmentFault {
                        pc:0x1000, address:0x2001, alignment:4, access:k
                    }) if k==access),
                        "{word:x} {level:?}: {result:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn duplex_loads_and_stores_fault_in_direct_execution_and_defer_in_the_lifter() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::{HexagonLifter, LiftContext, SmirLifter};
    for (word, access) in [
        (0x00120043_u32, MemoryAccessKind::Read),
        (0xa0120043, MemoryAccessKind::Write),
    ] {
        for endian in [Endianness::Little, Endianness::Big] {
            let (mut cpu, memory, mut before) = machine(word, 0x2001, 0x1000, endian);
            before.r[4] = 0x2001;
            cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
            assert_alignment(cpu.step_insn().unwrap_err(), 0x1000, 0x2001, 4, access);
            assert_eq!(regs(&cpu).r, before.r);
            let mut bytes = [0; 32];
            memory.read_slice(&mut bytes, GuestAddress(0x2000)).unwrap();
            assert_eq!(bytes, [0xa5; 32]);
        }
        // Duplex SMIR remains outside this lifter's supported profile. It must
        // reject the packet rather than emit an unchecked memory operation.
        lifter_stack::run_on_large_lifter_stack("Hexagon duplex admission", || {
            let result = HexagonLifter::default_isa().lift_insn(
                0x1000,
                &word.to_le_bytes(),
                &mut LiftContext::new(SourceArch::Hexagon),
            );
            assert!(
                matches!(result, Err(rax::smir::LiftError::Unsupported { .. })),
                "{result:?}"
            );
        });
    }
}
