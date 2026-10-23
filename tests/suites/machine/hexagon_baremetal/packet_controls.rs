use super::packet_memory::{lifted, machine, regs};
use rax::config::Endianness;
use rax::smir::ir::context::ArchRegState;
use rax::smir::ir::types::{ArchReg, HexagonReg};
use rax::smir::optimize::OptLevel;
use rax::smir::{BlockResult, ExitReason};
use rax::vm::vcpu::{CpuState, HexagonRegisters, VCpu};
use vm_memory::{Bytes, GuestAddress};
fn contract(
    words: &[u32],
    setup: impl Fn(&mut HexagonRegisters),
    r1: u32,
    r2: u32,
    control: usize,
    value: u32,
    stored: bool,
) {
    let (mut cpu, memory, mut before) = machine(words[0], 0x2000, 0x1000, Endianness::Little);
    setup(&mut before);
    for (n, word) in words.iter().enumerate() {
        memory
            .write_slice(&word.to_le_bytes(), GuestAddress(0x1000 + n as u64 * 4))
            .unwrap();
    }
    cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
    for _ in words.iter().filter(|word| (**word >> 14) & 3 == 3) {
        cpu.step_insn().unwrap();
    }
    let after = regs(&cpu);
    assert_eq!(
        (after.r[1], after.r[2], after.control(control)),
        (r1, r2, value),
        "direct"
    );
    if stored {
        let mut bytes = [0; 4];
        memory.read_slice(&mut bytes, GuestAddress(0x2000)).unwrap();
        assert_eq!(bytes, r1.to_le_bytes());
    }
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let (state, mut memory, result) = lifted(words, &before, level);
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Halt)),
            "{level:?}: {result:?}"
        );
        assert_eq!(
            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(1))),
            u64::from(r1),
            "{level:?}: r1"
        );
        assert_eq!(
            state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(2))),
            u64::from(r2),
            "{level:?}: r2"
        );
        let ArchRegState::Hexagon(hex) = &state.arch_regs else {
            unreachable!()
        };
        let actual = match control {
            0 => hex.sa[0],
            1 => hex.lc[0],
            2 => hex.sa[1],
            3 => hex.lc[1],
            4 => hex
                .p
                .iter()
                .enumerate()
                .fold(0_u32, |value, (index, byte)| {
                    value | (u32::from(*byte) << (8 * index))
                }),
            6 => hex.m[0],
            7 => hex.m[1],
            8 => hex.usr,
            11 => hex.gp,
            12 => hex.cs[0],
            13 => hex.cs[1],
            _ => unreachable!(),
        };
        assert_eq!(actual, value, "{level:?}: control");
        if stored {
            use rax::smir::ir::memory::SmirMemory;
            let mut bytes = [0; 4];
            memory.read(0x2000, &mut bytes).unwrap();
            assert_eq!(bytes, r1.to_le_bytes());
        }
    }
}
#[test]
fn gp_relative_load_uses_old_gp_and_commits_the_new_gp() {
    contract(
        &[0x6220400b, 0x4980c001],
        |r| {
            r.c[11] = 0x2000;
            r.r[0] = 0x3000;
        },
        0xa5a5a5a5,
        0x11223344,
        11,
        0x3000,
        false,
    );
}
#[test]
fn gp_relative_store_uses_old_gp_and_commits_the_new_gp() {
    contract(
        &[0x6220400b, 0x4880c100],
        |r| {
            r.c[11] = 0x2000;
            r.r[0] = 0x3000;
            r.r[1] = 0x76543210;
        },
        0x76543210,
        0x11223344,
        11,
        0x3000,
        true,
    );
}
#[test]
fn modifier_postincrement_uses_old_m0() {
    for selector in 0..2 {
        contract(
            &[
                0x62204006 + selector as u32,
                0x9d82c001 | ((selector as u32) << 13),
            ],
            |r| {
                r.c[6 + selector] = 4;
                r.r[0] = 8;
                r.r[2] = 0x2000;
            },
            0xa5a5a5a5,
            0x2004,
            6 + selector,
            8,
            false,
        );
    }
}

#[test]
fn circular_postincrement_uses_old_start() {
    for selector in 0..2 {
        contract(
            &[
                0x6220400c + selector as u32,
                0x9982c021 | ((selector as u32) << 13),
            ],
            |r| {
                r.c[6 + selector] = 16;
                r.c[12 + selector] = 0x2000;
                r.r[0] = 0x3000;
                r.r[2] = 0x200c;
            },
            0xa5a5a5a5,
            0x2000,
            12 + selector,
            0x3000,
            false,
        );
    }
}

#[test]
fn circular_postincrement_uses_old_modifier_length() {
    for selector in 0..2 {
        contract(
            &[
                0x62204006 + selector as u32,
                0x9982c021 | ((selector as u32) << 13),
            ],
            |r| {
                r.c[6 + selector] = 16;
                r.c[12 + selector] = 0x2000;
                r.r[0] = 32;
                r.r[2] = 0x200c;
            },
            0xa5a5a5a5,
            0x2000,
            6 + selector,
            32,
            false,
        );
    }
}

#[test]
fn scalar_control_reads_preserve_every_represented_value() {
    for index in [0, 1, 2, 3, 6, 7, 8, 11, 12, 13] {
        let value = if index == 11 { 0x12345640 } else { 0x12345678 };
        contract(
            &[0x6a00c001 | ((index as u32) << 16)],
            |r| {
                r.c[index] = value;
            },
            value,
            0x11223344,
            index,
            value,
            false,
        );
    }
}
#[test]
fn scalar_control_writes_commit_every_represented_value() {
    for index in [0, 1, 2, 3, 6, 7, 8, 11, 12, 13] {
        let value = if index == 11 { 0x12345640 } else { 0x12345678 };
        contract(
            &[0x6220c000 | index as u32],
            |r| {
                r.r[0] = 0x12345678;
            },
            0x2000,
            0x11223344,
            index,
            value,
            false,
        );
    }
}
#[test]
fn packed_predicate_control_read_preserves_all_four_bytes() {
    contract(
        &[0x6a04c001],
        |r| {
            r.p = [0x12, 0x34, 0x56, 0x78];
        },
        0x78563412,
        0x11223344,
        4,
        0x78563412,
        false,
    );
}
#[test]
fn packed_predicate_control_write_sets_all_four_bytes() {
    contract(
        &[0x6220c004],
        |r| {
            r.r[0] = 0x87654321;
        },
        0x2000,
        0x11223344,
        4,
        0x87654321,
        false,
    );
}
#[test]
fn scalar_pc_read_returns_the_packet_address() {
    contract(&[0x6a09c001], |_| {}, 0x1000, 0x11223344, 11, 0, false);
}
#[test]
fn scalar_pc_write_has_no_effect() {
    contract(
        &[0x6220c009],
        |r| {
            r.r[0] = 0x2000;
        },
        0x2000,
        0x11223344,
        11,
        0,
        false,
    );
}

#[path = "../../../support/hexagon.rs"]
mod lifter_stack;

#[test]
fn unrepresented_control_registers_reject_instead_of_using_system_stubs() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::LiftError;
    use rax::smir::{HexagonLifter, LiftContext, SmirLifter};
    lifter_stack::run_on_large_lifter_stack("Hexagon control admission", || {
        for index in [
            5_u32, 10, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
        ] {
            for word in [0x6a00c001 | (index << 16), 0x6220c000 | index] {
                let mut lifter = HexagonLifter::default_isa();
                let mut context = LiftContext::new(SourceArch::Hexagon);
                assert!(
                    matches!(
                        lifter.lift_insn(0x1000, &word.to_le_bytes(), &mut context),
                        Err(LiftError::Unsupported { addr: 0x1000, .. })
                    ),
                    "C{index}, word={word:x}"
                );
            }
        }
    });
}

#[test]
fn a_new_unmapped_gp_does_not_change_this_packets_valid_load_address() {
    contract(
        &[0x6220400b, 0x4980c001],
        |r| {
            r.c[11] = 0x2000;
            r.r[0] = 0x4000;
        },
        0xa5a5a5a5,
        0x11223344,
        11,
        0x4000,
        false,
    );
}

#[test]
fn an_old_unmapped_gp_faults_and_rolls_back_the_control_write() {
    for second in [0x4980c001_u32, 0x4880c100] {
        let words = [0x6220400b, second];
        let (mut cpu, memory, mut before) = machine(words[0], 0x2000, 0x1000, Endianness::Little);
        before.c[11] = 0x4000;
        before.r[0] = 0x2000;
        memory
            .write_slice(&words[1].to_le_bytes(), GuestAddress(0x1004))
            .unwrap();
        cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
        assert!(cpu.step_insn().is_err());
        let after = regs(&cpu);
        assert_eq!(after.r, before.r);
        assert_eq!(after.c, before.c);
        assert_eq!(after.p, before.p);
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let (state, mut memory, result) = lifted(&words, &before, level);
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::MemoryFault { addr: 0x4000, .. })
                ),
                "{level:?}: {result:?}"
            );
            assert_eq!(
                state.read_arch_reg(ArchReg::Hexagon(HexagonReg::Gp)),
                0x4000
            );
            assert_eq!(
                state.read_arch_reg(ArchReg::Hexagon(HexagonReg::R(1))),
                0x2000
            );
            use rax::smir::ir::memory::SmirMemory;
            let mut bytes = [0; 4];
            memory.read(0x2000, &mut bytes).unwrap();
            assert_eq!(bytes, [0xa5; 4]);
        }
    }
}

#[test]
fn usr_read_observes_packet_entry_while_saturation_commits_the_sticky_bit() {
    contract(
        &[0x6a084001, 0xf643c402],
        |r| {
            r.c[8] = 0x20000;
            r.r[3] = 0x7fffffff;
            r.r[4] = 1;
        },
        0x20000,
        0x7fffffff,
        8,
        0x20001,
        false,
    );
}

#[test]
fn packed_control_write_preserves_old_predicate_inputs_to_the_rest_of_the_packet() {
    contract(
        &[0x62204004, 0x4182c001],
        |r| {
            r.r[0] = 0xff;
            r.r[2] = 0x2000;
            r.p[0] = 0;
        },
        0x2000,
        0x2000,
        4,
        0xff,
        false,
    );
}

#[test]
fn packed_control_write_is_not_admitted_as_a_new_predicate_producer() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::LiftError;
    use rax::smir::{HexagonLifter, LiftContext, SmirLifter};
    lifter_stack::run_on_large_lifter_stack("Hexagon packed predicate new rejection", || {
        let mut lifter = HexagonLifter::default_isa();
        let mut context = LiftContext::new(SourceArch::Hexagon);
        lifter
            .lift_insn(0x1000, &0x62204004_u32.to_le_bytes(), &mut context)
            .unwrap();
        assert!(matches!(
            lifter.lift_insn(0x1004, &0x4382c001_u32.to_le_bytes(), &mut context),
            Err(LiftError::Unsupported { addr: 0x1004, mnemonic }) if mnemonic == "packed predicate control write is not a .new producer"
        ));
    });
}

#[test]
fn an_ordinary_compare_remains_a_valid_new_predicate_producer() {
    contract(
        &[0xf2034400, 0x4382c001],
        |r| {
            r.r[2] = 0x2000;
            r.r[3] = 7;
            r.r[4] = 7;
        },
        0xa5a5a5a5,
        0x2000,
        4,
        0xff,
        false,
    );
}

#[test]
fn packed_control_write_does_not_poison_a_later_packets_valid_new_producer() {
    contract(
        &[0x6220c004, 0xf2034400, 0x4382c001],
        |r| {
            r.r[0] = 0xff;
            r.r[2] = 0x2000;
            r.r[3] = 7;
            r.r[4] = 7;
        },
        0xa5a5a5a5,
        0x2000,
        4,
        0xff,
        false,
    );
}

#[test]
fn usr_read_in_the_next_packet_observes_the_committed_sticky_bit() {
    contract(
        &[0xf643c402, 0x6a08c001],
        |r| {
            r.c[8] = 0x20000;
            r.r[3] = 0x7fffffff;
            r.r[4] = 1;
        },
        0x20001,
        0x7fffffff,
        8,
        0x20001,
        false,
    );
}
