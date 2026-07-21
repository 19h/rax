//! Direct/native APX extended-EVEX legacy-prefix differentials.

use super::*;
use crate::vm::vcpu::VCpu;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const HLT_PC: u64 = 24;
const CODE: &[u8] = &[
    0x67, 0x62, 0xF4, 0xFC, 0x08, 0x03, 0x03, // add rax,qword ptr [ebx]
    0x64, 0x67, 0x62, 0xF4, 0xFC, 0x08, 0x03, 0x0E, // add rcx,fs:[esi]
    0x65, 0x62, 0xF4, 0xFC, 0x08, 0x03, 0x17, // add rdx,gs:[rdi]
    0xEB, 0x00, // jmp hlt
    0xF4,
];

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.fs.base = 0x5000;
    vcpu.sregs.gs.base = 0x6000;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x9000;
    vcpu.regs.rbp = 0x8000;
    vcpu.regs.rax = 0x10;
    vcpu.regs.rcx = 0x20;
    vcpu.regs.rdx = 0x30;
    vcpu.regs.rbx = 0xFFFF_FFFF_0000_4000;
    vcpu.regs.rsi = 0xFFFF_FFFF_0000_0100;
    vcpu.regs.rdi = 0x200;
    vcpu.regs.rflags = 0x2 | 0x08D5 | flags::bits::DF;
    vcpu.set_apx_enabled(true);
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);
    vcpu
}

fn run_direct_to_hlt(vcpu: &mut X86_64Vcpu) {
    for _ in 0..8 {
        if vcpu.regs.rip == HLT_PC {
            return;
        }
        assert!(
            vcpu.step()
                .expect("direct prefixed APX instruction")
                .is_none()
        );
    }
    panic!("direct execution did not reach HLT at {HLT_PC:#x}");
}

fn register_image(vcpu: &X86_64Vcpu) -> serde_json::Value {
    serde_json::to_value(vcpu.get_regs().expect("read materialized x86 registers"))
        .expect("serialize x86 register image")
}

#[test]
fn native_apx_prefix_addresses_match_direct_without_losing_wrap_or_segment_bases() {
    let memory = memory_with_code(CODE);
    for (address, value) in [
        (0x4000, 0x111_u64),
        (0x5100, 0x222),
        (0x6200, 0x333),
        (0x0100, 0xBAD1),
        (0x0200, 0xBAD2),
    ] {
        memory
            .write_slice(&value.to_le_bytes(), GuestAddress(address))
            .unwrap();
    }

    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    run_direct_to_hlt(&mut direct);

    let region = native
        .jit_compile_region()
        .expect("compile prefixed APX region")
        .expect("APX addr32 and FS/GS memory operands must remain native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(register_image(&native), register_image(&direct));
    assert_eq!(native.regs.rip, HLT_PC);
    assert_eq!(native.regs.rax, 0x121);
    assert_eq!(native.regs.rcx, 0x242);
    assert_eq!(native.regs.rdx, 0x363);
}

#[test]
fn direct_apx_permitted_prefix_groups_retire_register_forms_exactly() {
    for prefix in [0x67, 0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65] {
        let code = [prefix, 0x62, 0xF4, 0x7C, 0x08, 0x03, 0xC1, 0xF4];
        let mut vcpu = test_vcpu(memory_with_code(&code));

        assert!(
            vcpu.step()
                .unwrap_or_else(|error| panic!("prefix {prefix:02X}: {error:#}"))
                .is_none()
        );
        assert_eq!(vcpu.regs.rip, 7, "prefix {prefix:02X}");
        assert_eq!(vcpu.regs.rax, 0x30, "prefix {prefix:02X}");
    }
}

#[test]
fn direct_apx_prefix_legality_is_fail_closed_before_state_commit() {
    for (prefixes, name) in [
        (&[0x66][..], "operand-size"),
        (&[0xF2], "REPNE"),
        (&[0xF3], "REP"),
        (&[0xF0], "LOCK"),
        (&[0x40], "REX"),
        (&[0x40, 0x67], "REX hidden by address-size"),
        (&[0x48, 0x64], "REX hidden by segment"),
        (&[0xD5, 0x00], "REX2"),
    ] {
        let mut code = prefixes.to_vec();
        code.extend_from_slice(&[0x62, 0xF4, 0x7C, 0x08, 0x03, 0xC1, 0xF4]);
        let mut vcpu = test_vcpu(memory_with_code(&code));
        let before = register_image(&vcpu);
        let error = format!("{:#}", vcpu.step().expect_err(name));

        assert!(error.contains("IDT entry 6 not present"), "{name}: {error}");
        assert_eq!(vcpu.regs.rip, 0, "{name}: faulting RIP");
        assert_eq!(register_image(&vcpu), before, "{name}: state commit");
    }
}
