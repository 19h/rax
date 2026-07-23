//! Direct-execution regressions for terminal Intel APX MAP4 #UD forms.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::VCpu;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const MEMORY_SIZE: usize = 0x10000;

/// Opcode bytes assigned by Intel APX Architecture Specification revision 7.0:
/// the section 3.1.5 table plus the later-added MOVRS rows in section 6.38. An
/// assigned byte can still be reserved for a particular prefix or ModR/M value;
/// this predicate deliberately classifies only the first dispatch frontier.
fn apx_rev7_map4_opcode_is_assigned(opcode: u8) -> bool {
    matches!(
        opcode,
        0x00..=0x03
            | 0x08..=0x0B
            | 0x10..=0x13
            | 0x18..=0x1B
            | 0x20..=0x24
            | 0x28..=0x2C
            | 0x30..=0x33
            | 0x38..=0x3B
            | 0x40..=0x4F
            | 0x60
            | 0x61
            | 0x65
            | 0x66
            | 0x69
            | 0x6B
            | 0x80
            | 0x81
            | 0x83..=0x85
            | 0x88
            | 0x8A
            | 0x8B
            | 0x8F
            | 0xA5
            | 0xAD
            | 0xAF
            | 0xC0
            | 0xC1
            | 0xD0..=0xD3
            | 0xF0..=0xF2
            | 0xF4..=0xF9
            | 0xFC
            | 0xFE
            | 0xFF
    )
}

fn tail_code_vcpu(code: &[u8]) -> X86_64Vcpu {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), MEMORY_SIZE)]).unwrap());
    let rip = (MEMORY_SIZE - code.len()) as u64;
    memory.write_slice(code, GuestAddress(rip)).unwrap();

    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.regs.rip = rip;
    vcpu.regs.rsp = 0x9000;
    vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
    vcpu.regs.rbx = 0xFEDC_BA98_7654_3210;
    vcpu.regs.rflags = 0x2 | 0x8D5;
    vcpu.set_apx_enabled(true);
    vcpu
}

fn tail_vcpu(opcode: u8) -> X86_64Vcpu {
    tail_code_vcpu(&[0x62, 0xF4, 0x7C, 0x08, opcode])
}

#[test]
fn every_unassigned_apx_map4_opcode_raises_ud_before_modrm_fetch() {
    assert_eq!(
        (0..=u8::MAX)
            .filter(|opcode| apx_rev7_map4_opcode_is_assigned(*opcode))
            .count(),
        86,
        "Intel APX revision 7 assigns 86 distinct MAP4 opcode bytes"
    );

    for opcode in 0..=u8::MAX {
        if apx_rev7_map4_opcode_is_assigned(opcode) {
            continue;
        }

        let mut vcpu = tail_vcpu(opcode);
        let before = vcpu.regs.clone();
        let error = vcpu
            .step()
            .expect_err("unassigned APX MAP4 opcode must raise #UD");

        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "opcode {opcode:02X}: {error:?}"
        );
        assert_eq!(vcpu.regs.rax, before.rax, "opcode {opcode:02X}: RAX");
        assert_eq!(vcpu.regs.rbx, before.rbx, "opcode {opcode:02X}: RBX");
        assert_eq!(vcpu.regs.rsp, before.rsp, "opcode {opcode:02X}: RSP");
        assert_eq!(
            vcpu.regs.rflags, before.rflags,
            "opcode {opcode:02X}: RFLAGS"
        );
        assert_eq!(vcpu.regs.rip, before.rip, "opcode {opcode:02X}: fault RIP");
    }
}

#[test]
fn profile_disabled_apx_f8_forms_raise_ud_before_modrm_fetch() {
    for (name, code) in [
        ("unassigned NP F8", &[0x62, 0xF4, 0x7C, 0x08, 0xF8][..]),
        (
            "F2 F8 without ENQCMD or USER_MSR",
            &[0x62, 0xF4, 0x7F, 0x08, 0xF8][..],
        ),
        (
            "F3 F8 without ENQCMD or USER_MSR",
            &[0x62, 0xF4, 0x7E, 0x08, 0xF8][..],
        ),
    ] {
        let mut vcpu = tail_code_vcpu(code);
        let before = vcpu.regs.clone();
        let error = vcpu.step().expect_err(name);

        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "{name}: {error:?}"
        );
        assert_eq!(vcpu.regs.rax, before.rax, "{name}: RAX");
        assert_eq!(vcpu.regs.rbx, before.rbx, "{name}: RBX");
        assert_eq!(vcpu.regs.rsp, before.rsp, "{name}: RSP");
        assert_eq!(vcpu.regs.rflags, before.rflags, "{name}: RFLAGS");
        assert_eq!(vcpu.regs.rip, before.rip, "{name}: fault RIP");
    }
}

#[test]
fn rex2_lea_remains_available_in_legacy_map0() {
    // REX2 extends ordinary legacy-map0 LEA; it does not imply an EVEX MAP4
    // promotion. D5 48 8D 03 encodes LEA R16,[RBX].
    let mut vcpu = tail_code_vcpu(&[0xD5, 0x48, 0x8D, 0x03]);
    vcpu.regs.rbx = 0x1234_5678_9ABC_DEF0;

    assert!(vcpu.step().expect("REX2 LEA").is_none());
    assert_eq!(vcpu.regs.r16, 0x1234_5678_9ABC_DEF0);
    assert_eq!(vcpu.regs.rip, MEMORY_SIZE as u64);
}
