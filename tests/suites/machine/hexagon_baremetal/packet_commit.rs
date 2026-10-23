//! V68 section 3.3.2: packet-visible effects commit after all accesses succeed.
use super::packet_memory::{machine, regs};
use rax::config::Endianness;
use rax::vm::vcpu::{CpuState, VCpu};
use vm_memory::{Bytes, GuestAddress};

#[test]
fn direct_later_store_fault_preserves_earlier_ram() {
    for endian in [Endianness::Little, Endianness::Big] {
        let words = [0xa1814200_u32, 0xa184c200];
        let (mut cpu, memory, mut before) = machine(words[0], 0x2000, 0x1000, endian);
        before.r[4] = 0x5000;
        cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
        let code = match endian {
            Endianness::Little => words[1].to_le_bytes(),
            Endianness::Big => words[1].to_be_bytes(),
        };
        memory.write_slice(&code, GuestAddress(0x1004)).unwrap();
        assert!(cpu.step_insn().is_err());
        let mut data = [0; 32];
        memory.read_slice(&mut data, GuestAddress(0x2000)).unwrap();
        assert_eq!(data, [0xa5; 32], "{endian:?}");
        assert_eq!(regs(&cpu).r, before.r);
        assert_eq!(cpu.current_pc(), 0x1000);
        assert_eq!(cpu.instruction_count(), 0);
    }
}

#[test]
fn mmio_read_resume_retains_earlier_new_value_producers() {
    use rax::vm::vcpu::VcpuExit;
    let words = [0x78004038_u32, 0x91814019, 0xa1a4d400];
    let port = rax::devices::map::HEXAGON_DEBUG_MMIO_BASE;
    let (mut cpu, memory, mut before) = machine(words[0], port as u32, 0x1000, Endianness::Little);
    before.r[4] = 0x2000;
    cpu.set_state(&CpuState::hexagon(before)).unwrap();
    for (i, word) in words.iter().enumerate() {
        memory
            .write_slice(&word.to_le_bytes(), GuestAddress(0x1000 + i as u64 * 4))
            .unwrap();
    }
    assert!(
        matches!(cpu.step_insn().unwrap(), Some(VcpuExit::MmioRead { addr, size:4 }) if addr == port)
    );
    cpu.complete_io_in(&0x12345678_u32.to_le_bytes());
    assert!(cpu.step_insn().unwrap().is_none());
    let mut data = [0; 4];
    memory.read_slice(&mut data, GuestAddress(0x2000)).unwrap();
    assert_eq!(u32::from_le_bytes(data), 1);
    assert_eq!(regs(&cpu).r[25], 0x12345678);
    assert_eq!(cpu.instruction_count(), 1);
}

#[test]
fn direct_later_fault_rolls_back_usr_sticky_control_state() {
    let (mut cpu, memory, mut before) = machine(0x8c8240b8, 0x2000, 0x1000, Endianness::Little);
    before.r[2] = 0x80000000;
    before.r[4] = 0x5000;
    before.c[8] = 0x400;
    cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
    memory
        .write_slice(&0xa184c200_u32.to_le_bytes(), GuestAddress(0x1004))
        .unwrap();
    assert!(cpu.step_insn().is_err());
    assert_eq!(regs(&cpu).c, before.c);
    assert_eq!(regs(&cpu).r, before.r);
}

#[test]
fn faulting_packet_does_not_publish_an_earlier_mmio_write() {
    let port = rax::devices::map::HEXAGON_DEBUG_MMIO_BASE;
    let (mut cpu, memory, mut before) =
        machine(0xa1814200, port as u32, 0x1000, Endianness::Little);
    before.r[4] = 0x5000;
    cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
    memory
        .write_slice(&0xa184c200_u32.to_le_bytes(), GuestAddress(0x1004))
        .unwrap();
    let result = cpu.step_insn();
    assert!(result.is_err(), "{result:?}");
    assert_eq!(regs(&cpu).r, before.r);
    assert_eq!(cpu.instruction_count(), 0);
}

#[test]
fn successful_dual_store_commits_both_values_in_guest_order() {
    for endian in [Endianness::Little, Endianness::Big] {
        let (mut cpu, memory, mut before) = machine(0xa1814200, 0x2000, 0x1000, endian);
        before.r[4] = 0x2008;
        cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
        let end = match endian {
            Endianness::Little => 0xa184c200_u32.to_le_bytes(),
            Endianness::Big => 0xa184c200_u32.to_be_bytes(),
        };
        memory.write_slice(&end, GuestAddress(0x1004)).unwrap();
        assert!(cpu.step_insn().unwrap().is_none());
        let value = match endian {
            Endianness::Little => before.r[2].to_le_bytes(),
            Endianness::Big => before.r[2].to_be_bytes(),
        };
        let mut expected = [0xa5; 32];
        expected[..4].copy_from_slice(&value);
        expected[8..12].copy_from_slice(&value);
        let mut data = [0; 32];
        memory.read_slice(&mut data, GuestAddress(0x2000)).unwrap();
        assert_eq!(data, expected, "{endian:?}");
        assert_eq!(cpu.current_pc(), 0x1008);
        assert_eq!(cpu.instruction_count(), 1);
    }
}

#[test]
fn instruction_fetch_failure_discards_earlier_packet_store() {
    let (mut cpu, memory, _) = machine(0xa1814200, 0x2000, 0x3ffc, Endianness::Little);
    assert!(cpu.step_insn().is_err());
    let mut data = [0; 32];
    memory.read_slice(&mut data, GuestAddress(0x2000)).unwrap();
    assert_eq!(data, [0xa5; 32]);
    assert_eq!(cpu.current_pc(), 0x3ffc);
    assert_eq!(cpu.instruction_count(), 0);
}

#[test]
fn rollback_preserves_the_prior_successful_packet() {
    let words = [0xa181c200_u32, 0xa1814300, 0xa184c200];
    let (mut cpu, memory, mut before) = machine(words[0], 0x2000, 0x1000, Endianness::Little);
    before.r[4] = 0x5000;
    cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
    for (i, word) in words.iter().enumerate() {
        memory
            .write_slice(&word.to_le_bytes(), GuestAddress(0x1000 + i as u64 * 4))
            .unwrap();
    }
    assert!(cpu.step_insn().unwrap().is_none());
    assert!(cpu.step_insn().is_err());
    let mut value = [0; 4];
    memory.read_slice(&mut value, GuestAddress(0x2000)).unwrap();
    assert_eq!(u32::from_le_bytes(value), before.r[2]);
    assert_eq!(cpu.current_pc(), 0x1004);
    assert_eq!(cpu.instruction_count(), 1);
}

#[test]
fn mmio_read_keeps_packet_entry_state_until_a_complete_response() {
    use rax::vm::vcpu::VcpuExit;
    let port = rax::devices::map::HEXAGON_DEBUG_MMIO_BASE;
    let (mut cpu, _, before) = machine(0x9181c018, port as u32, 0x1000, Endianness::Little);
    for response in [&[][..], &[0x78][..]] {
        assert!(
            matches!(cpu.step_insn().unwrap(), Some(VcpuExit::MmioRead { addr, size: 4 }) if addr == port)
        );
        assert_eq!(cpu.current_pc(), 0x1000);
        assert_eq!(regs(&cpu).r, before.r);
        assert_eq!(cpu.instruction_count(), 0);
        cpu.complete_io_in(response);
    }
    assert!(matches!(
        cpu.step_insn().unwrap(),
        Some(VcpuExit::MmioRead { .. })
    ));
    cpu.complete_io_in(&0x12345678_u32.to_le_bytes());
    assert!(cpu.step_insn().unwrap().is_none());
    assert_eq!(regs(&cpu).r[24], 0x12345678);
    assert_eq!(cpu.current_pc(), 0x1004);
    assert_eq!(cpu.instruction_count(), 1);
}

#[test]
fn mmio_writes_are_delivered_once_before_the_single_packet_commit() {
    use rax::vm::vcpu::VcpuExit;
    let (mut cpu, memory, mut before) = machine(0xa1814200, 0xf0000000, 0x1000, Endianness::Little);
    before.r[4] = rax::devices::map::HEXAGON_DEBUG_MMIO_BASE as u32;
    cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
    memory
        .write_slice(&0xa184c200_u32.to_le_bytes(), GuestAddress(0x1004))
        .unwrap();
    for address in [0xf0000000, rax::devices::map::HEXAGON_DEBUG_MMIO_BASE] {
        let Some(VcpuExit::MmioWrite { addr, data }) = cpu.step_insn().unwrap() else {
            panic!("missing MMIO write");
        };
        assert_eq!(addr, address);
        assert_eq!(data, before.r[2].to_le_bytes());
        assert_eq!(cpu.current_pc(), 0x1000);
        assert_eq!(cpu.instruction_count(), 0);
    }
    assert!(cpu.step_insn().unwrap().is_none());
    assert_eq!(cpu.current_pc(), 0x1008);
    assert_eq!(cpu.instruction_count(), 1);
}

#[test]
fn a_ram_store_waits_until_the_packet_mmio_write_is_acknowledged() {
    use rax::vm::vcpu::VcpuExit;
    let (mut cpu, memory, mut before) = machine(0xa1814200, 0xf0000000, 0x1000, Endianness::Little);
    before.r[4] = 0x2000;
    cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
    memory
        .write_slice(&0xa184c200_u32.to_le_bytes(), GuestAddress(0x1004))
        .unwrap();
    assert!(matches!(
        cpu.step_insn().unwrap(),
        Some(VcpuExit::MmioWrite { .. })
    ));
    let mut value = [0; 4];
    memory.read_slice(&mut value, GuestAddress(0x2000)).unwrap();
    assert_eq!(value, [0xa5; 4]);
    assert!(cpu.step_insn().unwrap().is_none());
    memory.read_slice(&mut value, GuestAddress(0x2000)).unwrap();
    assert_eq!(u32::from_le_bytes(value), before.r[2]);
    assert_eq!(cpu.instruction_count(), 1);
}

#[test]
fn replacing_state_cancels_a_suspended_packet_and_its_response() {
    let port = rax::devices::map::HEXAGON_DEBUG_MMIO_BASE;
    let (mut cpu, memory, mut before) =
        machine(0x9181c018, port as u32, 0x1000, Endianness::Little);
    memory
        .write_slice(&0x7f00c000_u32.to_le_bytes(), GuestAddress(0x1004))
        .unwrap();
    assert!(cpu.step_insn().unwrap().is_some());
    before.set_pc(0x1004);
    before.r[24] = 0x88776655;
    cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
    cpu.complete_io_in(&0x12345678_u32.to_le_bytes());
    assert!(cpu.step_insn().unwrap().is_none());
    assert_eq!(regs(&cpu).r, before.r);
    assert_eq!(cpu.current_pc(), 0x1008);
    assert_eq!(cpu.instruction_count(), 1);
}

#[test]
fn vector_store_fault_does_not_publish_a_valid_prefix() {
    let (mut cpu, memory, mut before) = machine(0x2824c0e2, 0x2000, 0x1000, Endianness::Little);
    before.r[4] = 0x3fc0;
    before.v[2] = [0x11223344; 32];
    cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
    memory
        .write_slice(&[0xa5; 64], GuestAddress(0x3fc0))
        .unwrap();
    assert!(cpu.step_insn().is_err());
    let mut prefix = [0; 64];
    memory
        .read_slice(&mut prefix, GuestAddress(0x3fc0))
        .unwrap();
    assert_eq!(prefix, [0xa5; 64]);
    assert_eq!(regs(&cpu).v, before.v);
    assert_eq!(cpu.instruction_count(), 0);
}

#[test]
fn vector_scatter_fault_discards_all_prior_elements() {
    let (mut cpu, memory, mut before) = machine(0x2f24c182, 0x2000, 0x1000, Endianness::Little);
    before.r[4] = 0x2000;
    before.c[6] = 0x3000;
    before.v[1] = std::array::from_fn(|i| i as u32 * 4);
    before.v[1][31] = 0x3000;
    before.v[2] = std::array::from_fn(|i| i as u32 + 1);
    cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
    memory
        .write_slice(&[0xa5; 128], GuestAddress(0x2000))
        .unwrap();
    assert!(cpu.step_insn().is_err());
    let mut data = [0; 128];
    memory.read_slice(&mut data, GuestAddress(0x2000)).unwrap();
    assert_eq!(data, [0xa5; 128]);
    assert_eq!(regs(&cpu).v, before.v);
    assert_eq!(regs(&cpu).c, before.c);
    assert_eq!(cpu.instruction_count(), 0);
}

#[test]
fn scatter_accumulation_retains_all_same_address_increments() {
    let (mut cpu, memory, mut before) = machine(0x2f24c182, 0x2000, 0x1000, Endianness::Little);
    before.r[4] = 0x2000;
    before.c[6] = 3;
    before.v[1] = [0; 32];
    before.v[2] = std::array::from_fn(|i| i as u32 + 1);
    cpu.set_state(&CpuState::hexagon(before)).unwrap();
    assert!(cpu.step_insn().unwrap().is_none());
    let mut value = [0; 4];
    memory.read_slice(&mut value, GuestAddress(0x2000)).unwrap();
    assert_eq!(u32::from_le_bytes(value), 0xa5a5a5a5_u32.wrapping_add(528));
}

#[test]
fn later_alignment_fault_discards_earlier_store() {
    let (mut cpu, memory, mut before) = machine(0xa1814200, 0x2000, 0x1000, Endianness::Little);
    before.r[4] = 0x2001;
    cpu.set_state(&CpuState::hexagon(before)).unwrap();
    memory
        .write_slice(&0xa184c200_u32.to_le_bytes(), GuestAddress(0x1004))
        .unwrap();
    let error = cpu.step_insn().unwrap_err();
    super::packet_memory::assert_alignment(
        error,
        0x1000,
        0x2001,
        4,
        rax::error::MemoryAccessKind::Write,
    );
    let mut bytes = [0; 32];
    memory.read_slice(&mut bytes, GuestAddress(0x2000)).unwrap();
    assert_eq!(bytes, [0xa5; 32]);
    assert_eq!(cpu.instruction_count(), 0);
}

#[test]
fn control_register_transfer_is_rolled_back_on_a_later_fault() {
    let (mut cpu, memory, mut before) = machine(0x62224008, 0x2000, 0x1000, Endianness::Little);
    before.r[4] = 0x5000;
    before.c[8] = 0x400;
    cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
    memory
        .write_slice(&0xa184c200_u32.to_le_bytes(), GuestAddress(0x1004))
        .unwrap();
    assert!(cpu.step_insn().is_err());
    assert_eq!(regs(&cpu).c, before.c);
    assert_eq!(regs(&cpu).p, before.p);
}

#[test]
fn a_final_word_mmio_completion_never_executes_the_following_packet() {
    let port = rax::devices::map::HEXAGON_DEBUG_MMIO_BASE;
    let (mut cpu, memory, _) = machine(0x9181c018, port as u32, 0x1000, Endianness::Little);
    // The next packet writes R24, so over-retirement is visible in both PC and data.
    memory
        .write_slice(&0x7800c038_u32.to_le_bytes(), GuestAddress(0x1004))
        .unwrap();
    assert!(cpu.step_insn().unwrap().is_some());
    cpu.complete_io_in(&0x12345678_u32.to_le_bytes());
    assert!(cpu.step_insn().unwrap().is_none());
    assert_eq!(regs(&cpu).r[24], 0x12345678);
    assert_eq!(cpu.current_pc(), 0x1004);
    assert_eq!(cpu.instruction_count(), 1);
    assert!(cpu.step_insn().unwrap().is_none());
    assert_eq!(regs(&cpu).r[24], 1);
    assert_eq!(cpu.current_pc(), 0x1008);
    assert_eq!(cpu.instruction_count(), 2);
}

#[test]
fn fetch_fault_restores_the_packet_entry_reservation() {
    for prior_reservation in [false, true] {
        let pc = if prior_reservation { 0x3ff8 } else { 0x3ffc };
        let initial = if prior_reservation {
            0x9201c018
        } else {
            0x92054018
        };
        let (mut cpu, memory, mut before) = machine(initial, 0x2000, pc, Endianness::Little);
        before.r[5] = if prior_reservation { 0x2010 } else { 0x2000 };
        cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
        if prior_reservation {
            memory
                .write_slice(&0x92054018_u32.to_le_bytes(), GuestAddress(0x3ffc))
                .unwrap();
            assert!(cpu.step_insn().unwrap().is_none());
        }
        assert!(cpu.step_insn().is_err());
        memory
            .write_slice(&0xa0a1c200_u32.to_le_bytes(), GuestAddress(0x3ffc))
            .unwrap();
        assert!(cpu.step_insn().unwrap().is_none());
        assert_eq!(regs(&cpu).p[0], if prior_reservation { 0xff } else { 0 });
        let mut bytes = [0; 4];
        memory.read_slice(&mut bytes, GuestAddress(0x2000)).unwrap();
        assert_eq!(
            u32::from_le_bytes(bytes),
            if prior_reservation {
                before.r[2]
            } else {
                0xa5a5a5a5
            }
        );
        assert_eq!(
            cpu.instruction_count(),
            if prior_reservation { 2 } else { 1 }
        );
    }
}

#[test]
fn an_unterminated_packet_cannot_grow_the_write_log_past_four_words() {
    let (mut cpu, memory, _) = machine(0xa1814200, 0x2000, 0x1000, Endianness::Little);
    for offset in [4, 8, 12, 16] {
        memory
            .write_slice(&0x7f004000_u32.to_le_bytes(), GuestAddress(0x1000 + offset))
            .unwrap();
    }
    let error = cpu.step_insn().unwrap_err();
    assert!(error.to_string().contains("exceeds four words"));
    let mut bytes = [0; 32];
    memory.read_slice(&mut bytes, GuestAddress(0x2000)).unwrap();
    assert_eq!(bytes, [0xa5; 32]);
    assert_eq!(cpu.current_pc(), 0x1000);
    assert_eq!(cpu.instruction_count(), 0);
}

#[test]
fn bulk_accesses_cannot_bypass_mmio_routing_through_backing_ram() {
    use rax::config::HexagonIsa;
    use rax::isa::hexagon::HexagonVcpu;
    use rax::vm::vcpu::HexagonRegisters;
    use std::sync::Arc;
    use vm_memory::GuestMemoryMmap;
    let port = rax::devices::map::HEXAGON_DEBUG_MMIO_BASE;
    let memory = Arc::new(
        GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x4000), (GuestAddress(port), 0x1000)])
            .unwrap(),
    );
    memory
        .write_slice(&0x2824c0e2_u32.to_le_bytes(), GuestAddress(0x1000))
        .unwrap();
    memory
        .write_slice(&[0xa5; 128], GuestAddress(port))
        .unwrap();
    let mut before = HexagonRegisters::default();
    before.set_pc(0x1000);
    before.r[4] = port as u32;
    before.v[2] = [0x11223344; 32];
    let mut cpu = HexagonVcpu::new(0, memory.clone(), HexagonIsa::V68, Endianness::Little);
    cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
    let error = cpu.step_insn().unwrap_err();
    assert!(error.to_string().contains("MMIO access is not supported"));
    let mut bytes = [0; 128];
    memory.read_slice(&mut bytes, GuestAddress(port)).unwrap();
    assert_eq!(bytes, [0xa5; 128]);
    assert_eq!(regs(&cpu).v, before.v);
    assert_eq!(cpu.instruction_count(), 0);
}

#[test]
fn memory_noshuf_forwards_full_and_partial_overlapping_stores() {
    for endian in [Endianness::Little, Endianness::Big] {
        for (load, expected) in [
            (0x9184c018_u32, 0x80ff7f01),
            (
                0x9144c038,
                if endian == Endianness::Little {
                    0xffff80ff
                } else {
                    0x7f01
                },
            ),
        ] {
            let (mut cpu, memory, mut before) = machine(0xa1814200, 0x2000, 0x1000, endian);
            before.r[2] = 0x80ff7f01;
            before.r[4] = 0x2000;
            cpu.set_state(&CpuState::hexagon(before)).unwrap();
            let code = match endian {
                Endianness::Little => load.to_le_bytes(),
                Endianness::Big => load.to_be_bytes(),
            };
            memory.write_slice(&code, GuestAddress(0x1004)).unwrap();
            assert!(cpu.step_insn().unwrap().is_none());
            assert_eq!(regs(&cpu).r[24], expected, "{endian:?}, {load:#x}");
        }
    }
}

#[test]
fn a_load_before_a_store_reads_the_old_memory_value() {
    let (mut cpu, memory, mut before) = machine(0x91844018, 0x2000, 0x1000, Endianness::Little);
    before.r[4] = 0x2000;
    cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
    memory
        .write_slice(&0xa181c200_u32.to_le_bytes(), GuestAddress(0x1004))
        .unwrap();
    assert!(cpu.step_insn().unwrap().is_none());
    assert_eq!(regs(&cpu).r[24], 0xa5a5a5a5);
    let mut bytes = [0; 4];
    memory.read_slice(&mut bytes, GuestAddress(0x2000)).unwrap();
    assert_eq!(u32::from_le_bytes(bytes), before.r[2]);
}

#[test]
fn queued_code_stores_do_not_replace_packet_entry_instruction_words() {
    let (mut cpu, memory, mut before) = machine(0xa1814200, 0x1004, 0x1000, Endianness::Little);
    before.r[2] = 0x7f00c000;
    cpu.set_state(&CpuState::hexagon(before)).unwrap();
    memory
        .write_slice(&0x7800c038_u32.to_le_bytes(), GuestAddress(0x1004))
        .unwrap();
    assert!(cpu.step_insn().unwrap().is_none());
    assert_eq!(regs(&cpu).r[24], 1);
    assert_eq!(cpu.current_pc(), 0x1008);
    let mut bytes = [0; 4];
    memory.read_slice(&mut bytes, GuestAddress(0x1004)).unwrap();
    assert_eq!(u32::from_le_bytes(bytes), 0x7f00c000);
}

#[test]
fn unsupported_ordered_mmio_write_read_does_not_reverse_device_accesses() {
    let port = rax::devices::map::HEXAGON_DEBUG_MMIO_BASE;
    let (mut cpu, memory, mut before) =
        machine(0xa1814200, port as u32, 0x1000, Endianness::Little);
    before.r[4] = port as u32;
    cpu.set_state(&CpuState::hexagon(before.clone())).unwrap();
    memory
        .write_slice(&0x9184c018_u32.to_le_bytes(), GuestAddress(0x1004))
        .unwrap();
    let error = cpu.step_insn().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("ordered MMIO write/read packet is not supported")
    );
    assert_eq!(regs(&cpu).r, before.r);
    assert_eq!(cpu.current_pc(), 0x1000);
    assert_eq!(cpu.instruction_count(), 0);
}

#[test]
fn a_gp_transfer_does_not_replace_a_same_packet_old_gp_address() {
    let (mut cpu, memory, mut before) = machine(0x6222400b, 0x2000, 0x1000, Endianness::Little);
    before.r[2] = 0x3000;
    before.c[11] = 0x2000;
    cpu.set_state(&CpuState::hexagon(before)).unwrap();
    memory
        .write_slice(&0x4980c018_u32.to_le_bytes(), GuestAddress(0x1004))
        .unwrap();
    assert!(cpu.step_insn().unwrap().is_none());
    let after = regs(&cpu);
    assert_eq!(after.r[24], 0xa5a5a5a5);
    assert_eq!(after.c[11], 0x3000);
    assert_eq!(cpu.instruction_count(), 1);
}
