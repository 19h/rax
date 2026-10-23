//! V68 sections 3.3.1/3.3.5: packet-old GPR reads and conditional-write merging.
use super::packet_memory::{lifted, machine, regs};
use rax::config::Endianness;
use rax::smir::ir::memory::SmirMemory;
use rax::smir::ir::types::{ArchReg, HexagonReg};
use rax::smir::optimize::OptLevel;
use rax::smir::{BlockResult, ExitReason};
use rax::vm::vcpu::{CpuState, HexagonRegisters, VCpu};
use vm_memory::{Bytes, GuestAddress};

#[path = "../../../support/hexagon.rs"]
mod lifter_stack;

fn contract(
    words: &[u32],
    packets: usize,
    setup: impl Fn(&mut HexagonRegisters),
    changes: &[(usize, u32)],
    new_store: bool,
) {
    let (mut cpu, memory, mut before) = machine(words[0], 0x2000, 0x1000, Endianness::Little);
    before.r[24] = 0x11;
    before.r[25] = 0x22;
    setup(&mut before);
    cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
    for (index, word) in words.iter().enumerate() {
        memory
            .write_slice(&word.to_le_bytes(), GuestAddress(0x1000 + index as u64 * 4))
            .unwrap();
    }
    let mut expected = before.r;
    for &(index, value) in changes {
        expected[index] = value;
    }
    for _ in 0..packets {
        assert!(cpu.step_insn().unwrap().is_none());
    }
    assert_eq!(regs(&cpu).r, expected, "direct");
    let mut expected_memory = [0xa5; 32];
    if new_store {
        expected_memory[..4].copy_from_slice(&0x3000_u32.to_le_bytes());
    }
    let mut after = [0; 32];
    memory.read_slice(&mut after, GuestAddress(0x2000)).unwrap();
    assert_eq!(after, expected_memory);
    let mut untouched = [0; 4];
    memory
        .read_slice(&mut untouched, GuestAddress(0x3000))
        .unwrap();
    assert_eq!(untouched, [0; 4]);
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let (state, mut memory, result) = lifted(words, &before, level);
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            "{level:?}: {result:?}"
        );
        for index in 0..32 {
            assert_eq!(
                state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(index))),
                expected[index as usize] as u64,
                "{level:?}, r{index}"
            );
        }
        memory.read(0x2000, &mut after).unwrap();
        assert_eq!(after, expected_memory, "{level:?}");
        memory.read(0x3000, &mut untouched).unwrap();
        assert_eq!(untouched, [0; 4]);
    }
}

#[test]
fn packet_swap_reads_both_old_values() {
    contract(
        &[0x70794018, 0x7078c019],
        1,
        |_| {},
        &[(24, 0x22), (25, 0x11)],
        false,
    );
}
#[test]
fn packet_arithmetic_reads_old_inputs() {
    contract(
        &[0xb0194038, 0xb018c059],
        1,
        |_| {},
        &[(24, 0x23), (25, 0x13)],
        false,
    );
}
#[test]
fn ordinary_load_uses_the_old_address_register() {
    contract(
        &[0x000040c0, 0x78004001, 0x9181c018],
        1,
        |_| {},
        &[(1, 0x3000), (24, 0xa5a5a5a5)],
        false,
    );
}
#[test]
fn new_store_data_and_old_address_can_alias_one_register() {
    contract(
        &[0x000040c0, 0x78004001, 0xa1a1d200],
        1,
        |_| {},
        &[(1, 0x3000)],
        true,
    );
}
#[test]
fn opaque_scalar_alu_reads_the_old_packet_input() {
    contract(
        &[0x8c9a4098, 0x8c98c099],
        1,
        |r| {
            r.r[24] = (-3_i32) as u32;
            r.r[26] = 4;
        },
        &[(24, 4), (25, 3)],
        false,
    );
}
#[test]
fn pair_combine_captures_both_sources_before_either_write() {
    contract(&[0xf518d918], 1, |_| {}, &[(24, 0x22), (25, 0x11)], false);
}
#[test]
fn packet_boundary_replaces_the_old_register_snapshot() {
    contract(
        &[0x7079c018, 0x7078c019],
        2,
        |_| {},
        &[(24, 0x22), (25, 0x22)],
        false,
    );
}
#[test]
fn conditional_writes_merge_only_the_executed_result() {
    for predicate in [0, 0xff] {
        contract(
            &[0x7e004038, 0x7e80c058],
            1,
            |r| {
                r.p[0] = predicate;
            },
            &[(24, if predicate == 0 { 2 } else { 1 })],
            false,
        );
    }
}
#[test]
fn later_packet_memory_uses_the_committed_address() {
    contract(
        &[0x000040c0, 0x7800c001, 0x9181c018],
        2,
        |_| {},
        &[(1, 0x3000), (24, 0)],
        false,
    );
}

fn branch_contract(words: &[u32], before: HexagonRegisters, take: bool) {
    flow_contract(
        words,
        before,
        if take { 0x1010 } else { 0x1008 },
        Some(take),
    );
}

fn flow_contract(
    words: &[u32],
    before: HexagonRegisters,
    expected_pc: u64,
    branch_taken: Option<bool>,
) {
    use rax::smir::ir::types::{Address, FunctionId, MemWidth, SourceArch};
    use rax::smir::ir::{FunctionBuilder, Terminator, TrapKind};
    use rax::smir::{ControlFlow, HexagonLifter, LiftContext, OpKind, SmirLifter};
    let (mut cpu, memory, _) = machine(words[0], 0x2000, 0x1000, Endianness::Little);
    for (index, word) in words.iter().enumerate() {
        memory
            .write_slice(&word.to_le_bytes(), GuestAddress(0x1000 + index as u64 * 4))
            .unwrap();
    }
    let frame_data = [0x44, 0x33, 0x22, 0x11, 0x8b, 0x77, 0x66, 0x55];
    memory
        .write_slice(&frame_data, GuestAddress(0x2000))
        .unwrap();
    let mut before = before;
    before.set_pc(0x1000);
    cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
    cpu.step_insn().unwrap();
    assert_eq!(cpu.current_pc(), expected_pc, "direct");
    if branch_taken.is_none() {
        let mut expected = before.r;
        if expected_pc != 0x1004 {
            expected[2] = 0x11223344;
            expected[3] = 0x5566778b;
            expected[29] = 0x2008;
        }
        assert_eq!(regs(&cpu).r, expected, "direct raw frame registers");
    }
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        lifter_stack::run_on_large_lifter_stack("Hexagon register branch contract", || {
            let mut lifter = HexagonLifter::default_isa();
            let mut context = LiftContext::new(SourceArch::Hexagon);
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            let mut observed_flow = None;
            for (index, word) in words.iter().enumerate() {
                let result = lifter
                    .lift_insn(0x1000 + index as u64 * 4, &word.to_le_bytes(), &mut context)
                    .unwrap();
                if let ControlFlow::CondBranchReg {
                    cond,
                    taken,
                    not_taken,
                } = result.control_flow
                {
                    assert_eq!(taken, 0x1010);
                    assert_eq!(not_taken, 0x1008);
                    observed_flow = Some((cond, MemWidth::B1));
                } else if let ControlFlow::IndirectBranch { target } = result.control_flow {
                    observed_flow = Some((target, MemWidth::B4));
                }
                for op in result.ops {
                    builder.push_op(op.guest_pc, op.kind);
                }
            }
            // Keep the actual control-flow value live under optimization.
            let (flow_value, flow_width) = observed_flow.unwrap();
            builder.push_op(
                0x1000,
                OpKind::Store {
                    src: flow_value,
                    addr: Address::Absolute(0x3ff0),
                    width: flow_width,
                },
            );
            builder.set_terminator(Terminator::Trap {
                kind: TrapKind::Halt,
            });
            let mut function = builder.finish();
            rax::smir::optimize::optimize_function(&mut function, level);
            let mut state = rax::smir::SmirContext::new_hexagon();
            for index in 0..32 {
                state.write_arch_reg(
                    ArchReg::Hexagon(HexagonReg::R(index)),
                    before.r[index as usize] as u64,
                );
            }
            for index in 0..4 {
                state.write_arch_reg(
                    ArchReg::Hexagon(HexagonReg::P(index)),
                    before.p[index as usize] as u64,
                );
            }
            let mut memory = rax::smir::FlatMemory::new(0x4000);
            memory.write(0x2000, &frame_data).unwrap();
            let result = rax::smir::SmirInterpreter::new().execute_block(
                &mut state,
                &mut memory,
                function.entry_block().unwrap(),
            );
            assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
            let mut value = [0; 4];
            memory.read(0x3ff0, &mut value).unwrap();
            if let Some(take) = branch_taken {
                assert_eq!(value[0] != 0, take, "{level:?}");
            } else {
                assert_eq!(u32::from_le_bytes(value) as u64, expected_pc, "{level:?}");
            }
            let actual = regs(&cpu);
            for index in 0..32 {
                assert_eq!(
                    state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(index))),
                    actual.r[index as usize] as u64,
                    "{level:?}, r{index}"
                );
            }
        });
    }
}
#[test]
fn explicit_new_compare_observes_its_producer() {
    for other in [0, 1] {
        let mut before = HexagonRegisters::default();
        before.r[25] = other;
        branch_contract(&[0x78004038, 0x2002d908], before, other == 1);
    }
}
#[test]
fn new_compare_source_does_not_change_its_ordinary_alias_operand() {
    for old in [0, 1] {
        let mut before = HexagonRegisters::default();
        before.r[24] = old;
        branch_contract(&[0x78004038, 0x2002d808], before, old == 1);
    }
}
#[test]
fn ordinary_compound_compare_reads_the_old_source() {
    for other in [0, 1] {
        let mut before = HexagonRegisters::default();
        before.r[2] = other;
        branch_contract(&[0x78004021, 0x1401c208], before, other == 0);
    }
}

#[test]
fn predicated_pair_load_freezes_an_aliasing_address_register() {
    contract(
        &[0x41c2c002],
        1,
        |r| {
            r.r[2] = 0x2000;
            r.p[0] = 0xff;
        },
        &[(2, 0xa5a5a5a5), (3, 0xa5a5a5a5)],
        false,
    );
}

#[test]
fn deallocframe_uses_the_old_frame_address_and_updates_gpr_aliases() {
    contract(
        &[0x901ec01e],
        1,
        |r| {
            r.r[29] = 0x2010;
            r.r[30] = 0x2000;
        },
        &[(29, 0x2008), (30, 0xa5a5a5a5), (31, 0xa5a5a5a5)],
        false,
    );
}

#[test]
fn context_sp_fp_lr_aliases_share_their_general_register_values() {
    for (number, alias) in [
        (29, HexagonReg::Sp),
        (30, HexagonReg::Fp),
        (31, HexagonReg::Lr),
    ] {
        let mut state = rax::smir::SmirContext::new_hexagon();
        state.write_arch_reg(ArchReg::Hexagon(HexagonReg::R(number)), 0x12345678);
        assert_eq!(state.read_arch_reg(ArchReg::Hexagon(alias)), 0x12345678);
        state.write_arch_reg(ArchReg::Hexagon(alias), 0x87654321);
        assert_eq!(
            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(number))),
            0x87654321
        );
    }
}

#[test]
fn raw_deallocframe_writes_its_encoded_pair_and_sp() {
    contract(
        &[0x9001c002],
        1,
        |r| {
            r.r[29] = 0x2010;
        },
        &[(2, 0xa5a5a5a5), (3, 0xa5a5a5a5), (29, 0x2008)],
        false,
    );
}

#[test]
fn raw_dealloc_return_branches_to_the_loaded_high_word() {
    let mut before = HexagonRegisters::default();
    before.r[1] = 0x2000;
    before.r[29] = 0x2010;
    before.r[31] = 0x777;
    flow_contract(&[0x9601c002], before, 0x55667788, None);
}

#[test]
fn predicated_raw_dealloc_return_cancels_all_effects_when_false() {
    for predicate in [0, 0xff] {
        let mut before = HexagonRegisters::default();
        before.r[1] = 0x2000;
        before.r[29] = 0x2010;
        before.r[31] = 0x777;
        before.p[0] = predicate;
        flow_contract(
            &[0x9601d002],
            before,
            if predicate == 0 { 0x1004 } else { 0x55667788 },
            None,
        );
    }
}
