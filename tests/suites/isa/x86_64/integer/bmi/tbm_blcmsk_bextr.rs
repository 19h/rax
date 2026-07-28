//! Architectural coverage for AMD TBM BLCMSK and immediate-control BEXTR.

use crate::common::*;
use rax::isa::x86_64::flags;
use rax::vm::vcpu::Registers;

#[test]
fn blcmsk_matches_xor_with_increment_for_both_widths() {
    for (code, src, mask) in [
        (
            &[0x8F, 0xE9, 0x78, 0x02, 0xCB, 0xF4][..],
            0xFFFF_FFFD_u64,
            u64::from(u32::MAX),
        ),
        (
            &[0x8F, 0xE9, 0xF8, 0x02, 0xCB, 0xF4][..],
            0x0123_4567_89AB_CDEF,
            u64::MAX,
        ),
    ] {
        let mut initial = Registers::default();
        initial.rbx = src;
        initial.rax = u64::MAX;
        let (mut vcpu, _) = setup_tbm_vm(code, Some(initial));
        let regs = run_until_hlt(&mut vcpu).expect("execute BLCMSK");
        let truncated = src & mask;
        assert_eq!(regs.rax, (truncated ^ truncated.wrapping_add(1)) & mask);
    }
}

#[test]
fn immediate_bextr_extracts_controlled_bit_field_and_sets_flags() {
    // BEXTR EAX,EBX,0x0804: start=4, length=8.
    let code = [0x8F, 0xEA, 0x78, 0x10, 0xC3, 0x04, 0x08, 0x00, 0x00, 0xF4];
    let mut initial = Registers::default();
    initial.rbx = 0xCAFE_BABE;
    initial.rflags = 0x2 | flags::bits::CF | flags::bits::OF | flags::bits::PF;
    let (mut vcpu, _) = setup_tbm_vm(&code, Some(initial));
    let regs = run_until_hlt(&mut vcpu).expect("execute immediate BEXTR");

    assert_eq!(regs.rax, 0xAB);
    assert_eq!(
        regs.rflags & (flags::bits::CF | flags::bits::OF | flags::bits::ZF),
        0
    );
    assert_ne!(
        regs.rflags & flags::bits::PF,
        0,
        "unaffected flags must survive"
    );
}

#[test]
fn immediate_bextr_handles_zero_and_out_of_range_controls() {
    for (control, expected) in [
        (0x0000_u32, 0_u64),
        (0x0840, 0),
        (0x4004, 0x00FE_DCBA_9876_5432),
        (0x0804, 0x32),
    ] {
        let mut code = vec![0x8F, 0xEA, 0xF8, 0x10, 0xC3];
        code.extend_from_slice(&control.to_le_bytes());
        code.push(0xF4);
        let mut initial = Registers::default();
        initial.rbx = 0x0FED_CBA9_8765_4321;
        let (mut vcpu, _) = setup_tbm_vm(&code, Some(initial));
        let regs = run_until_hlt(&mut vcpu).expect("execute immediate BEXTR edge");
        assert_eq!(regs.rax, expected, "control={control:#06x}");
        assert_eq!(
            regs.rflags & flags::bits::ZF != 0,
            expected == 0,
            "control={control:#06x}"
        );
    }
}

#[test]
fn immediate_bextr_rip_relative_address_accounts_for_imm32() {
    // The 13-byte instruction ends at 0x100D; disp32=0x0FF3 selects 0x2000.
    let code = [
        0x8F, 0xEA, 0x78, 0x10, 0x05, 0xF3, 0x0F, 0x00, 0x00, 0x04, 0x08, 0x00, 0x00, 0xF4,
    ];
    let (mut vcpu, memory) = setup_tbm_vm(&code, None);
    write_mem_u32(&memory, 0x1234_5AB0);
    let regs = run_until_hlt(&mut vcpu).expect("execute RIP-relative immediate BEXTR");
    assert_eq!(regs.rax, 0xAB);
}

#[test]
fn removed_vex_tbm_aliases_raise_ud_without_committing() {
    for opcode in [0x01, 0x02] {
        let code = [0xC4, 0xE2, 0x78, opcode, 0xCB, 0xF4];
        let mut initial = Registers::default();
        initial.rax = 0x0123_4567_89AB_CDEF;
        initial.rbx = 0xFEDC_BA98_7654_3210;
        initial.rflags = 0x2 | flags::bits::CF | flags::bits::OF;
        let (mut vcpu, _) = setup_vm_no_idt(&code, Some(initial));

        let before = vcpu.get_regs().expect("read initial registers");
        let error = vcpu
            .step()
            .expect_err("unassigned VEX map-0F38 cell must raise #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "opcode={opcode:#04x}: expected #UD delivery failure, got {error}"
        );
        let after = vcpu.get_regs().expect("read fault registers");
        assert_eq!(after.rip, before.rip, "opcode={opcode:#04x}");
        assert_eq!(after.rax, before.rax, "opcode={opcode:#04x}");
        assert_eq!(after.rbx, before.rbx, "opcode={opcode:#04x}");
        assert_eq!(after.rflags, before.rflags, "opcode={opcode:#04x}");
    }
}
