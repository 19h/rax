use rax::config::{Endianness, HexagonIsa};
use rax::isa::hexagon::{
    HexagonVcpu,
    decode::{DecodedInsn, decode},
};
use rax::vm::vcpu::{CpuState, HexagonRegisters, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn machine(base: u32, words: &[u32], regs: HexagonRegisters) -> HexagonVcpu {
    let memory = Arc::new(
        GuestMemoryMmap::from_ranges(&[(GuestAddress((base & !0xfff) as u64), 0x1000)]).unwrap(),
    );
    for (i, word) in words.iter().enumerate() {
        memory
            .write_slice(
                &word.to_le_bytes(),
                GuestAddress(base as u64 + i as u64 * 4),
            )
            .unwrap();
    }
    let mut cpu = HexagonVcpu::new(0, memory, HexagonIsa::V68, Endianness::Little);
    let mut regs = regs;
    regs.set_pc(base);
    cpu.set_state(&CpuState::hexagon(regs)).unwrap();
    cpu
}

#[test]
fn ordinary_branch_offsets_use_the_packet_start_at_every_legal_word_position() {
    for (preceding, branch, offset) in
        [(1, 0x5800c004, 8), (2, 0x5800c008, 16), (3, 0x5800c010, 32)]
    {
        assert!(
            matches!(decode(branch, None, HexagonIsa::V68).insn, DecodedInsn::Jump {offset: x} if x == offset)
        );
        let mut words = vec![0xf2014200; preceding];
        words.push(branch);
        let mut cpu = machine(0x1000, &words, HexagonRegisters::default());
        assert!(cpu.step_insn().unwrap().is_none());
        assert_eq!(cpu.current_pc(), 0x1000 + offset as u64);
    }
}

#[test]
fn packet_relative_branch_wraps_at_the_32_bit_address_boundary() {
    let mut cpu = machine(
        0xfffffff8,
        &[0xf2014200, 0x5800c004],
        HexagonRegisters::default(),
    );
    assert!(cpu.step_insn().unwrap().is_none());
    assert_eq!(cpu.current_pc(), 0);
}

use rax::smir::ir::types::SourceArch;
use rax::smir::lift::hexagon::HexagonLifter;
use rax::smir::lift::{ControlFlow, LiftContext, SmirLifter};

fn lifted_target(base: u32, words: &[u32]) -> u64 {
    let mut lifter = HexagonLifter::default_isa();
    let mut ctx = LiftContext::new(SourceArch::Hexagon);
    let mut target = None;
    for (i, word) in words.iter().enumerate() {
        let result = lifter
            .lift_insn(base as u64 + i as u64 * 4, &word.to_le_bytes(), &mut ctx)
            .unwrap();
        if let ControlFlow::Branch { target: next } = result.control_flow {
            target = Some(next);
        }
    }
    target.expect("unconditional branch")
}

#[test]
fn lifted_branches_use_the_packet_start_at_every_legal_word_position() {
    for (preceding, branch, offset) in
        [(1, 0x5800c004, 8), (2, 0x5800c008, 16), (3, 0x5800c010, 32)]
    {
        let mut words = vec![0xf2014200; preceding];
        words.push(branch);
        assert_eq!(lifted_target(0x1000, &words), 0x1000 + offset);
    }
}

#[test]
fn lifted_packet_relative_branch_wraps_at_the_32_bit_address_boundary() {
    assert_eq!(lifted_target(0xfffffff8, &[0xf2014200, 0x5800c004]), 0);
}

#[test]
fn extended_and_single_word_branches_keep_their_packet_relative_targets() {
    // LLVM 23.0.0git, hexagonv68: { jump ##0x100 }.
    for (words, target) in [
        (&[0x00004004, 0x5800c000][..], 0x1100),
        (&[0x5800c004][..], 0x1008),
    ] {
        let mut cpu = machine(0x1000, words, HexagonRegisters::default());
        assert!(cpu.step_insn().unwrap().is_none());
        assert_eq!(cpu.current_pc(), target);
        assert_eq!(lifted_target(0x1000, words), target);
    }
}
