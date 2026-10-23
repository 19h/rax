//! Native cache-maintenance boundaries and legacy-mode fallback.

use super::*;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn cpu_with_code(code: &[u8]) -> X86_64Vcpu {
    let memory = Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    let mut cpu = X86_64Vcpu::new(0, memory);
    cpu.sregs.efer = 1 << 10;
    cpu.sregs.cs.l = true;
    cpu.sregs.cs.selector = 0;
    cpu.sregs.cr0 = 1 | (1 << 3);
    cpu.regs.rip = 0;
    cpu.regs.rax = 0x1234;
    cpu.regs.rsp = 0x8000;
    cpu.regs.rflags = 0x8d7;
    cpu.set_jit_mem(false);
    cpu.set_jit_call(false);
    cpu
}

#[test]
fn native_cache_maintenance_stops_at_the_exact_next_instruction() {
    for instruction in [&[0x0f, 0x08][..], &[0x0f, 0x09], &[0xf3, 0x0f, 0x09]] {
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0xb8, 0xad, 0x0b, 0, 0, 0xeb, 0, 0xf4]);
        let mut cpu = cpu_with_code(&code);
        let region = cpu
            .jit_compile_region()
            .unwrap()
            .expect("native cache operation");
        cpu.jit_run_region_native(&region);
        assert_eq!(cpu.regs.rip, instruction.len() as u64);
        assert_eq!(cpu.regs.rax, 0x1234);
        assert_eq!(cpu.sregs.cr0, 1 | (1 << 3));
        assert_eq!(cpu.regs.rflags, 0x8d7);
    }
}

#[test]
fn native_cache_maintenance_defers_legacy_instruction_pointer_rules() {
    for instruction in [&[0x0f, 0x08][..], &[0x0f, 0x09], &[0xf3, 0x0f, 0x09]] {
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0xeb, 0, 0xf4]);
        for default32 in [false, true] {
            let mut cpu = cpu_with_code(&code);
            cpu.sregs.cs.l = false;
            cpu.sregs.cs.db = default32;
            assert!(
                cpu.jit_compile_region().unwrap().is_none(),
                "{instruction:x?} default32={default32}"
            );
        }
    }
}
