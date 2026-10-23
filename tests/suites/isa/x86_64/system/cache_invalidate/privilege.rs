//! Cache maintenance privilege and prefix contracts (Intel SDM Vol. 2).

use crate::common::*;

const ENCODINGS: &[&[u8]] = &[&[0x0f, 0x08], &[0x0f, 0x09], &[0xf3, 0x0f, 0x09]];

#[test]
fn cache_maintenance_faults_at_nonzero_effective_cpl_before_advancing() {
    for &code in ENCODINGS {
        for (selector, vm) in [(1, false), (2, false), (3, false), (0, true)] {
            let (mut cpu, _) = setup_vm_no_idt(code, None);
            let mut sregs = cpu.get_sregs().unwrap();
            sregs.cs.selector = selector;
            sregs.cr0 |= 1 | (1 << 3);
            let cr0 = sregs.cr0;
            cpu.set_sregs(&sregs).unwrap();
            let mut regs = cpu.get_regs().unwrap();
            regs.rax = 0x1122334455667788;
            if vm {
                regs.rflags |= 1 << 17;
            }
            cpu.set_regs(&regs).unwrap();
            let error = cpu.step().expect_err("cache maintenance must raise #GP(0)");
            assert!(
                error.to_string().contains("IDT entry 13 not present"),
                "{code:02x?} CPL={selector} VM={vm}: {error}"
            );
            assert_eq!(cpu.get_regs().unwrap().rip, CODE_ADDR);
            assert_eq!(cpu.get_regs().unwrap().rax, regs.rax);
            assert_eq!(cpu.get_sregs().unwrap().cr0, cr0);
        }
    }
}

#[test]
fn cache_maintenance_preserves_cr0_ts_and_flags_at_cpl0_and_in_real_mode() {
    for &code in ENCODINGS {
        for protected in [false, true] {
            let (mut cpu, _) = setup_vm_no_idt(code, None);
            let mut sregs = cpu.get_sregs().unwrap();
            sregs.cr0 |= 1 << 3;
            if !protected {
                sregs.cr0 &= !1;
                sregs.cs.selector = 3;
            }
            let cr0 = sregs.cr0;
            cpu.set_sregs(&sregs).unwrap();
            let mut regs = cpu.get_regs().unwrap();
            regs.rax = 0x8877665544332211;
            regs.rflags = 0xc43;
            cpu.set_regs(&regs).unwrap();
            cpu.step().unwrap();
            let after = cpu.get_regs().unwrap();
            assert_eq!(after.rip, CODE_ADDR + code.len() as u64);
            assert_eq!(after.rax, regs.rax);
            assert_eq!(after.rflags, regs.rflags);
            assert_eq!(cpu.get_sregs().unwrap().cr0, cr0);
        }
    }
}

#[test]
fn lock_cache_maintenance_is_undefined_before_its_privilege_check() {
    for opcode in [8, 9] {
        let (mut cpu, _) = setup_vm_no_idt(&[0xf0, 0x0f, opcode], None);
        let mut sregs = cpu.get_sregs().unwrap();
        sregs.cs.selector = 3;
        cpu.set_sregs(&sregs).unwrap();
        let error = cpu.step().expect_err("LOCK cache maintenance must #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{error}"
        );
        assert_eq!(cpu.get_regs().unwrap().rip, CODE_ADDR);
    }
}

#[test]
fn cache_maintenance_ignored_prefixes_preserve_instruction_length_and_state() {
    for opcode in [8, 9] {
        for prefix in [
            0x26, 0x2e, 0x36, 0x3e, 0x64, 0x65, 0x66, 0x67, 0x40, 0x41, 0x42, 0x44, 0x48, 0x4f,
            0xf2, 0xf3,
        ] {
            let (mut cpu, _) = setup_vm_no_idt(&[prefix, 0x0f, opcode], None);
            let before = cpu.get_regs().unwrap();
            let cr0 = cpu.get_sregs().unwrap().cr0;
            cpu.step().unwrap();
            let after = cpu.get_regs().unwrap();
            assert_eq!(after.rip, CODE_ADDR + 3);
            assert_eq!(after.rflags, before.rflags);
            assert_eq!(cpu.get_sregs().unwrap().cr0, cr0);
        }
    }
}

#[test]
fn cache_maintenance_delivers_gp_zero_with_the_original_user_instruction_pc() {
    for &code in ENCODINGS {
        let (mut cpu, memory) = setup_vm(code, None);
        let mut sregs = cpu.get_sregs().unwrap();
        sregs.cs.selector = 0x1b;
        sregs.cs.dpl = 3;
        sregs.ss.selector = 0x23;
        sregs.ss.dpl = 3;
        let cr0 = sregs.cr0;
        cpu.set_sregs(&sregs).unwrap();
        let mut regs = cpu.get_regs().unwrap();
        regs.rsp = 0xa000;
        regs.rflags = 0xc43;
        cpu.set_regs(&regs).unwrap();
        cpu.step().unwrap();
        let after = cpu.get_regs().unwrap();
        assert_eq!(after.rip, INT_HANDLER_ADDR);
        let error: u64 = memory.read_obj(GuestAddress(after.rsp)).unwrap();
        let fault_pc: u64 = memory.read_obj(GuestAddress(after.rsp + 8)).unwrap();
        let saved_flags: u64 = memory.read_obj(GuestAddress(after.rsp + 24)).unwrap();
        assert_eq!(u64::from_le(error), 0);
        assert_eq!(u64::from_le(fault_pc), CODE_ADDR);
        assert_eq!(u64::from_le(saved_flags) & 0xcd5, regs.rflags & 0xcd5);
        assert_eq!(cpu.get_sregs().unwrap().cr0, cr0);
    }
}
