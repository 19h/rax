//! Direct/native differentials for state-backed x87 no-wait controls.

use super::*;
use crate::vm::vcpu::VCpu;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const STACK: u64 = 0x8000;

#[derive(Debug, PartialEq, Eq)]
struct FpuImage {
    control_word: u16,
    status_word: u16,
    tag_word: u16,
    data_ptr: u64,
    instr_ptr: u64,
    last_opcode: u16,
    st: [u64; 8],
    top: u8,
}

fn fpu_image(vcpu: &X86_64Vcpu) -> FpuImage {
    FpuImage {
        control_word: vcpu.fpu.control_word,
        status_word: vcpu.fpu.status_word,
        tag_word: vcpu.fpu.tag_word,
        data_ptr: vcpu.fpu.data_ptr,
        instr_ptr: vcpu.fpu.instr_ptr,
        last_opcode: vcpu.fpu.last_opcode,
        st: vcpu.fpu.st.map(f64::to_bits),
        top: vcpu.fpu.top,
    }
}

fn register_image(vcpu: &X86_64Vcpu) -> serde_json::Value {
    serde_json::to_value(vcpu.get_regs().expect("read materialized x86 registers"))
        .expect("serialize x86 register image")
}

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
    vcpu.sregs.cs.selector = 0;
    vcpu.sregs.cr0 = 0x21;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = STACK;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rax = 0xA5A5_5A5A_DEAD_BEEF;
    vcpu.regs.rbx = 0x1122_3344_5566_7788;
    vcpu.regs.rcx = 0x8877_6655_4433_2211;
    vcpu.regs.rdx = 0x0123_4567_89AB_CDEF;
    vcpu.regs.r16 = 0x1616_1616_1616_1616;
    vcpu.regs.r31 = 0x3131_3131_3131_3131;
    vcpu.regs.rflags = 0x2 | 0x08D5 | flags::bits::DF;
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);
    seed_fpu(&mut vcpu);
    vcpu
}

fn seed_fpu(vcpu: &mut X86_64Vcpu) {
    vcpu.fpu.control_word = 0x027F;
    vcpu.fpu.status_word = (5 << 11) | 0x87FF;
    vcpu.fpu.tag_word = 0x6996;
    vcpu.fpu.data_ptr = 0x1122_3344_5566_7788;
    vcpu.fpu.instr_ptr = 0x8877_6655_4433_2211;
    vcpu.fpu.last_opcode = 0x05A5;
    vcpu.fpu.st = std::array::from_fn(|index| {
        f64::from_bits(0x3FF0_0000_0000_0000 | ((index as u64) << 40) | index as u64)
    });
    vcpu.fpu.top = 5;
}

fn run_direct_to(vcpu: &mut X86_64Vcpu, target: u64) {
    for _ in 0..16 {
        if vcpu.regs.rip == target {
            return;
        }
        assert!(
            vcpu.step().expect("direct x87 control sequence").is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct x87 execution did not reach {target:#x}");
}

fn exception_without_idt(vcpu: &mut X86_64Vcpu) -> String {
    format!(
        "{:#}",
        vcpu.step()
            .expect_err("exception delivery must fail against the empty test IDT")
    )
}

#[test]
fn jit_x87_no_wait_controls_match_direct_for_every_ignored_prefix_class() {
    for (name, instruction) in [
        ("FNCLEX", &[0xDB, 0xE2][..]),
        ("FNINIT", &[0xDB, 0xE3][..]),
        ("FNSTSW AX", &[0xDF, 0xE0][..]),
    ] {
        for prefix in [None, Some(0x66), Some(0xF2), Some(0xF3)] {
            let encoded = prefix
                .into_iter()
                .chain(instruction.iter().copied())
                .collect::<Vec<_>>();
            let mut code = encoded.clone();
            code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
            let hlt_pc = encoded.len() as u64 + 2;
            let mut direct = test_vcpu(memory_with_code(&code));
            let mut native = test_vcpu(memory_with_code(&code));

            run_direct_to(&mut direct, hlt_pc);
            let region = native
                .jit_compile_region()
                .unwrap_or_else(|error| panic!("{name}, {prefix:?}: compile error: {error:?}"))
                .unwrap_or_else(|| panic!("{name}, {prefix:?}: must be native eligible"));
            assert!(region.uses_x87_environment_state, "{name}, {prefix:?}");
            assert!(!region.uses_mmx, "{name}, {prefix:?}");
            native.jit_run_region_native(&region);

            assert_eq!(
                register_image(&native),
                register_image(&direct),
                "{name}, {prefix:?}: register state"
            );
            assert_eq!(
                fpu_image(&native),
                fpu_image(&direct),
                "{name}, {prefix:?}: x87 state"
            );
            assert_eq!(native.regs.rip, hlt_pc, "{name}, {prefix:?}: frontier");
        }
    }
}

#[test]
fn jit_x87_cr0_em_ts_guard_is_dynamic_precise_and_noncommitting() {
    for fault_bits in [1 << 2, 1 << 3, (1 << 2) | (1 << 3)] {
        for (name, instruction) in [
            ("FNCLEX", &[0xDB, 0xE2][..]),
            ("FNINIT", &[0xDB, 0xE3][..]),
            ("FNSTSW AX", &[0xDF, 0xE0][..]),
        ] {
            let mut code = instruction.to_vec();
            code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
            let mut native = test_vcpu(memory_with_code(&code));

            let region = native
                .jit_compile_region()
                .unwrap_or_else(|error| panic!("{name}: compile error: {error:?}"))
                .unwrap_or_else(|| panic!("{name}: CR0 guard must remain native"));
            native.sregs.cr0 |= fault_bits;
            let registers_before = register_image(&native);
            let fpu_before = fpu_image(&native);
            native.jit_run_region_native(&region);

            assert_eq!(native.regs.rip, 0, "{name}, CR0={fault_bits:#x}");
            assert_eq!(
                register_image(&native),
                registers_before,
                "{name}, CR0={fault_bits:#x}: register commit"
            );
            assert_eq!(
                fpu_image(&native),
                fpu_before,
                "{name}, CR0={fault_bits:#x}: x87 commit"
            );
            let error = exception_without_idt(&mut native);
            assert!(
                error.contains("IDT entry 7 not present"),
                "{name}, CR0={fault_bits:#x}: expected #NM, got {error}"
            );
        }
    }
}

#[test]
fn x87_encoding_faults_precede_cr0_device_not_available() {
    for (name, instruction) in [
        ("FNCLEX", &[0xDB, 0xE2][..]),
        ("FNINIT", &[0xDB, 0xE3][..]),
        ("FNSTSW AX", &[0xDF, 0xE0][..]),
    ] {
        let mut locked = vec![0xF0];
        locked.extend_from_slice(instruction);
        let mut direct = test_vcpu(memory_with_code(&locked));
        direct.sregs.cr0 |= 1 << 3;
        let registers_before = register_image(&direct);
        let fpu_before = fpu_image(&direct);
        let error = exception_without_idt(&mut direct);
        assert!(
            error.contains("IDT entry 6 not present"),
            "LOCK {name}: expected #UD before #NM, got {error}"
        );
        assert_eq!(register_image(&direct), registers_before, "LOCK {name}");
        assert_eq!(fpu_image(&direct), fpu_before, "LOCK {name}");

        let mut rex2 = vec![0xD5, 0x00];
        rex2.extend_from_slice(instruction);
        rex2.extend_from_slice(&[0xEB, 0x00, 0xF4]);

        let mut apx_disabled = test_vcpu(memory_with_code(&rex2));
        apx_disabled.sregs.cr0 |= 1 << 3;
        apx_disabled.set_apx_enabled(true);
        let region = apx_disabled
            .jit_compile_region()
            .unwrap_or_else(|compile| panic!("REX2 {name}: {compile:?}"))
            .unwrap_or_else(|| panic!("REX2 {name}: guarded form must be native"));
        apx_disabled.set_apx_enabled(false);
        let registers_before = register_image(&apx_disabled);
        let fpu_before = fpu_image(&apx_disabled);
        apx_disabled.jit_run_region_native(&region);
        assert_eq!(apx_disabled.regs.rip, 0, "REX2 {name}: APX frontier");
        assert_eq!(
            register_image(&apx_disabled),
            registers_before,
            "REX2 {name}"
        );
        assert_eq!(fpu_image(&apx_disabled), fpu_before, "REX2 {name}");
        let error = exception_without_idt(&mut apx_disabled);
        assert!(
            error.contains("IDT entry 6 not present"),
            "REX2 {name}: expected #UD before #NM, got {error}"
        );

        let mut apx_enabled = test_vcpu(memory_with_code(&rex2));
        apx_enabled.sregs.cr0 |= 1 << 3;
        apx_enabled.set_apx_enabled(true);
        let region = apx_enabled
            .jit_compile_region()
            .unwrap_or_else(|compile| panic!("REX2 {name}: {compile:?}"))
            .unwrap_or_else(|| panic!("REX2 {name}: guarded form must be native"));
        apx_enabled.jit_run_region_native(&region);
        assert_eq!(apx_enabled.regs.rip, 0, "REX2 {name}: x87 frontier");
        let error = exception_without_idt(&mut apx_enabled);
        assert!(
            error.contains("IDT entry 7 not present"),
            "REX2 {name}: expected #NM with APX enabled, got {error}"
        );
    }
}

#[test]
fn jit_callout_round_trips_complete_x87_environment_and_payload_ownership() {
    const CODE: &[u8] = &[
        0xDB, 0xE2, // fnclex
        0xE8, 0x05, 0x00, 0x00, 0x00, // call callee at 0x0c
        0xDF, 0xE0, // fnstsw ax
        0xEB, 0x00, // jmp hlt
        0xF4, // hlt
        0xD9, 0xE8, // callee: fld1
        0xC3, // ret
    ];
    let mut direct = test_vcpu(memory_with_code(CODE));
    let mut native = test_vcpu(memory_with_code(CODE));
    for vcpu in [&mut direct, &mut native] {
        vcpu.set_jit_call(true);
        vcpu.fpu.init();
        vcpu.fpu.status_word = 0x80FF;
        vcpu.fpu.st = std::array::from_fn(|index| {
            f64::from_bits(0x4000_0000_0000_0000 | ((index as u64) << 40))
        });
    }

    run_direct_to(&mut direct, 0x0B);
    let region = native
        .jit_compile_region()
        .expect("compile x87 call-through region")
        .expect("state-backed x87 controls around CALL must be native eligible");
    assert!(region.uses_x87_environment_state);
    assert_eq!(region.callout_boundaries, vec![(2, 7)]);
    native.jit_run_region_native(&region);

    assert_eq!(register_image(&native), register_image(&direct));
    assert_eq!(fpu_image(&native), fpu_image(&direct));
    assert_eq!(native.regs.rip, 0x0B);
    assert_eq!(native.fpu.top, 7);
    assert_eq!(native.fpu.status_word, 7 << 11);
    assert_eq!(native.fpu.tag_word, 0x3FFF);
    assert_eq!(native.fpu.st[7].to_bits(), 1.0f64.to_bits());
    assert_eq!(native.regs.rax & 0xFFFF, 7 << 11);
}
