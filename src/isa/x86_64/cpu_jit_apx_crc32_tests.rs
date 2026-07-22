//! Native execution regressions for APX-promoted CRC32.

use super::*;
use crate::vm::vcpu::VCpu;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

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
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x9000;
    // AF is omitted because some user-mode x86 emulators do not preserve it
    // through PUSHFQ/POPFQ; all other modeled arithmetic flags remain live.
    vcpu.regs.rflags = 0x2 | 0x08C5 | flags::bits::DF;
    vcpu.set_apx_enabled(true);
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);
    vcpu
}

fn register_image(vcpu: &X86_64Vcpu) -> serde_json::Value {
    serde_json::to_value(vcpu.get_regs().expect("read materialized x86 registers"))
        .expect("serialize x86 register image")
}

#[test]
fn native_apx_crc32_matches_direct_for_register_and_memory_forms() {
    const HLT_PC: u64 = 35;
    const CODE: &[u8] = &[
        0x62, 0xEC, 0x7C, 0x08, 0xF0, 0xE1, // crc32 r20d,r17b
        0x62, 0xEC, 0x7D, 0x08, 0xF1, 0xE1, // crc32 r20d,r17w
        0x62, 0xEC, 0x7C, 0x08, 0xF1, 0xE1, // crc32 r20d,r17d
        0x62, 0xEC, 0xFC, 0x08, 0xF1, 0xE1, // crc32 r20,r17
        0x64, 0x62, 0xEC, 0xF8, 0x08, 0xF1, 0x64, 0x91, 0x20, // fs:[r17+r18*4+32]
        0xEB, 0x00, // jmp hlt
        0xF4,
    ];
    let memory = memory_with_code(CODE);
    let data = 0xA1B2_C3D4_E5F6_0718_u64;
    memory
        .write_slice(&data.to_le_bytes(), GuestAddress(0x412C))
        .unwrap();

    let setup = |vcpu: &mut X86_64Vcpu| {
        vcpu.sregs.fs.base = 0x4000;
        vcpu.regs.r17 = 0x100;
        vcpu.regs.r18 = 3;
        vcpu.regs.r20 = 0xFFFF_FFFF_1020_3040;
    };
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    setup(&mut direct);
    setup(&mut native);

    for _ in 0..8 {
        if direct.regs.rip == HLT_PC {
            break;
        }
        assert!(direct.step().expect("direct APX CRC32 sequence").is_none());
    }
    assert_eq!(direct.regs.rip, HLT_PC);

    let region = native
        .jit_compile_region()
        .expect("compile APX CRC32 region")
        .expect("guarded APX CRC32 must remain native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(register_image(&native), register_image(&direct));
    assert_eq!(native.regs.rip, HLT_PC);
}

#[test]
fn native_apx_crc32_state_backed_memory_covers_all_widths_and_stack_registers() {
    const DATA_ADDRESS: u64 = 0x200;
    const HLT_PC: u64 = 8;
    const DATA: u64 = 0xA1B2_C3D4_E5F6_0718;
    const ACCUMULATOR: u64 = 0xFFFF_FFFF_1020_3040;

    // Encodings independently produced by LLVM's APX extended-EVEX encoder.
    for (instruction, destination, name) in [
        ([0x62, 0xFC, 0x7C, 0x08, 0xF0, 0x21], 4, "ESP,m8"),
        ([0x62, 0xFC, 0x7D, 0x08, 0xF1, 0x29], 5, "EBP,m16"),
        ([0x62, 0xEC, 0x7C, 0x08, 0xF1, 0x21], 20, "R20D,m32"),
        ([0x62, 0x6C, 0xFC, 0x08, 0xF1, 0x39], 31, "R31,m64"),
    ] {
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
        let memory = memory_with_code(&code);
        memory
            .write_slice(&DATA.to_le_bytes(), GuestAddress(DATA_ADDRESS))
            .unwrap();

        let setup = |vcpu: &mut X86_64Vcpu| {
            vcpu.regs.r17 = DATA_ADDRESS;
            vcpu.set_reg(destination, ACCUMULATOR, 8);
        };
        let mut direct = test_vcpu(memory.clone());
        let mut native = test_vcpu(memory);
        setup(&mut direct);
        setup(&mut native);

        assert!(
            direct
                .step()
                .unwrap_or_else(|error| panic!("{name} direct CRC32: {error:#}"))
                .is_none()
        );
        assert!(direct.step().expect("direct jump to HLT").is_none());
        assert_eq!(direct.regs.rip, HLT_PC, "{name} direct frontier");

        let region = native
            .jit_compile_region()
            .unwrap_or_else(|error| panic!("{name} compile: {error:#}"))
            .unwrap_or_else(|| panic!("{name} must enter the state-backed native tier"));
        native.jit_run_region_native(&region);

        assert_eq!(
            register_image(&native),
            register_image(&direct),
            "{name} architectural state"
        );
        assert_eq!(
            native.get_reg(destination, 8) >> 32,
            0,
            "{name} zero extension"
        );
        assert_eq!(native.regs.r17, DATA_ADDRESS, "{name} address source");
        assert_eq!(native.regs.rip, HLT_PC, "{name} native frontier");
    }
}

#[test]
fn native_apx_crc32_memory_uses_precommit_destination_as_address_base() {
    const DATA_ADDRESS: u64 = 0x200;
    const DATA: u64 = 0xA1B2_C3D4_E5F6_0718;

    // Encodings independently produced by LLVM's APX extended-EVEX encoder.
    // Each destination is also the memory base, so native lowering must form
    // the address from the old architectural value before committing CRC32.
    for (instruction, destination, name) in [
        (
            &[0x62, 0xF4, 0x7C, 0x08, 0xF0, 0x24, 0x24][..],
            4,
            "ESP,[RSP]",
        ),
        (
            &[0x62, 0xF4, 0x7D, 0x08, 0xF1, 0x6D, 0x00][..],
            5,
            "EBP,[RBP]",
        ),
        (
            &[0x62, 0xEC, 0x7C, 0x08, 0xF1, 0x24, 0x24][..],
            20,
            "R20D,[R20]",
        ),
        (&[0x62, 0x4C, 0xFC, 0x08, 0xF1, 0x3F][..], 31, "R31,[R31]"),
    ] {
        let hlt_pc = instruction.len() as u64 + 2;
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
        let memory = memory_with_code(&code);
        memory
            .write_slice(&DATA.to_le_bytes(), GuestAddress(DATA_ADDRESS))
            .unwrap();

        let setup = |vcpu: &mut X86_64Vcpu| vcpu.set_reg(destination, DATA_ADDRESS, 8);
        let mut direct = test_vcpu(memory.clone());
        let mut native = test_vcpu(memory);
        setup(&mut direct);
        setup(&mut native);

        assert!(
            direct
                .step()
                .unwrap_or_else(|error| panic!("{name} direct CRC32: {error:#}"))
                .is_none()
        );
        assert!(direct.step().expect("direct jump to HLT").is_none());
        assert_eq!(direct.regs.rip, hlt_pc, "{name} direct frontier");

        let region = native
            .jit_compile_region()
            .unwrap_or_else(|error| panic!("{name} compile: {error:#}"))
            .unwrap_or_else(|| panic!("{name} alias must remain native eligible"));
        native.jit_run_region_native(&region);

        assert_eq!(
            register_image(&native),
            register_image(&direct),
            "{name} architectural state"
        );
        assert_eq!(native.regs.rip, hlt_pc, "{name} native frontier");
    }
}

#[test]
fn native_apx_crc32_rechecks_apx_before_commit_and_replays_direct_ud() {
    let code = [0x62, 0xEC, 0xFC, 0x08, 0xF1, 0xE1, 0xEB, 0x00, 0xF4];
    let mut vcpu = test_vcpu(memory_with_code(&code));
    vcpu.regs.r17 = 0x0123_4567_89AB_CDEF;
    vcpu.regs.r20 = 0xFFFF_FFFF_1020_3040;
    let region = vcpu
        .jit_compile_region()
        .expect("compile dynamically guarded APX CRC32")
        .expect("APX CRC32 guard must not block native admission");

    vcpu.set_apx_enabled(false);
    let before = register_image(&vcpu);
    vcpu.jit_run_region_native(&region);
    assert_eq!(register_image(&vcpu), before);
    assert_eq!(vcpu.regs.rip, 0);

    let error = format!("{:#}", vcpu.step().expect_err("APX-disabled CRC32 replay"));
    assert!(error.contains("IDT entry 6 not present"), "{error}");
    assert_eq!(register_image(&vcpu), before);
}

#[test]
fn native_apx_crc32_memory_fault_is_precise_and_noncommitting() {
    let code = [0x62, 0xEC, 0x7C, 0x08, 0xF1, 0x21, 0xEB, 0x00, 0xF4];
    let mut vcpu = test_vcpu(memory_with_code(&code));
    vcpu.regs.r20 = 0xFFFF_FFFF_1020_3040;
    vcpu.regs.r17 = 0x2_0000;
    let region = vcpu
        .jit_compile_region()
        .expect("compile faulting APX CRC32")
        .expect("APX CRC32 memory source must use native memory helpers");
    let before = register_image(&vcpu);

    vcpu.jit_run_region_native(&region);
    assert_eq!(register_image(&vcpu), before);
    assert_eq!(vcpu.regs.rip, 0);
}
