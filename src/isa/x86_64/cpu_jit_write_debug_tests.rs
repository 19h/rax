//! Native x86-64 JIT differentials for MOV-to-debug-register state.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const RFLAGS_VM: u64 = 1 << 17;

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
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.sregs.cr4 = 0x20;
    vcpu.sregs.dr0 = 0x1111;
    vcpu.sregs.dr1 = 0x2222;
    vcpu.sregs.dr2 = 0x3333;
    vcpu.sregs.dr3 = 0x4444;
    vcpu.sregs.dr6 = 0x400;
    vcpu.sregs.dr7 = 0x400;
    vcpu.regs.rax = 0x1111_2222_3333_4444;
    vcpu.regs.rcx = 0x2222_3333_4444_5555;
    vcpu.regs.rdx = 0x3333_4444_5555_6666;
    vcpu.regs.rbx = 0x4444_5555_6666_7777;
    vcpu.regs.rsi = 0xFFFF_0FF0;
    vcpu.regs.rdi = 0x400;
    vcpu.regs.r14 = 0xFFFF_0FF0;
    vcpu.regs.r15 = 0x400;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn run_direct_to(vcpu: &mut X86_64Vcpu, target: u64) {
    for _ in 0..32 {
        if vcpu.regs.rip == target {
            return;
        }
        assert!(
            vcpu.step().expect("direct MOV-to-DR sequence").is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct execution did not reach {target:#x}");
}

fn debug_state(vcpu: &X86_64Vcpu) -> [u64; 6] {
    [
        vcpu.sregs.dr0,
        vcpu.sregs.dr1,
        vcpu.sregs.dr2,
        vcpu.sregs.dr3,
        vcpu.sregs.dr6,
        vcpu.sregs.dr7,
    ]
}

#[test]
fn jit_mov_to_debug_registers_matches_direct_for_every_selector_and_alias() {
    let memory = memory_with_code(&[
        0x0F, 0x23, 0xC0, // mov dr0,rax
        0x0F, 0x23, 0xC9, // mov dr1,rcx
        0x0F, 0x23, 0xD2, // mov dr2,rdx
        0x0F, 0x23, 0xDB, // mov dr3,rbx
        0x0F, 0x23, 0xE6, // mov dr4,rsi (DR6 alias)
        0x0F, 0x23, 0xEF, // mov dr5,rdi (DR7 alias)
        0x41, 0x0F, 0x23, 0xF6, // mov dr6,r14
        0x41, 0x0F, 0x23, 0xFF, // mov dr7,r15
        0xEB, 0x00, // jmp hlt
        0xF4,
    ]);
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);

    run_direct_to(&mut direct, 28);
    let region = native
        .jit_compile_region()
        .expect("compile MOV-to-DR region")
        .expect("MOV-to-DR region must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(debug_state(&native), debug_state(&direct));
    assert_eq!(native.sregs.dr0, native.regs.rax);
    assert_eq!(native.sregs.dr1, native.regs.rcx);
    assert_eq!(native.sregs.dr2, native.regs.rdx);
    assert_eq!(native.sregs.dr3, native.regs.rbx);
    assert_eq!(native.sregs.dr6, native.regs.r14);
    assert_eq!(native.sregs.dr7, native.regs.r15);
    assert_eq!(native.regs.rsp, direct.regs.rsp);
    assert_eq!(native.regs.rbp, direct.regs.rbp);
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, direct.regs.rip);
}

#[test]
fn jit_mov_to_debug_register_handles_rsp_rbp_sources() {
    let memory = memory_with_code(&[
        0x0F, 0x23, 0xE4, // mov dr4,rsp (DR6 alias)
        0x0F, 0x23, 0xED, // mov dr5,rbp (DR7 alias)
        0xEB, 0x00, 0xF4,
    ]);
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);

    run_direct_to(&mut direct, 8);
    let region = native
        .jit_compile_region()
        .expect("compile stack-register MOV-to-DR region")
        .expect("state-backed RSP/RBP sources must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.sregs.dr6, direct.sregs.dr6);
    assert_eq!(native.sregs.dr7, direct.sregs.dr7);
    assert_eq!(native.sregs.dr6, 0x8000);
    assert_eq!(native.sregs.dr7, 0x7000);
    assert_eq!(native.regs.rsp, direct.regs.rsp);
    assert_eq!(native.regs.rbp, direct.regs.rbp);
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 8);
}

#[test]
fn jit_verify_compares_and_adopts_mov_to_debug_state() {
    let memory = memory_with_code(&[0x0F, 0x23, 0xD8, 0xEB, 0x00, 0xF4]);
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
    let region = vcpu
        .jit_compile_region()
        .expect("compile verified MOV-to-DR region")
        .expect("verified MOV-to-DR region must be native eligible");

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.sregs.dr3, 0x0123_4567_89AB_CDEF);
    assert_eq!(vcpu.regs.rip, 5);
}

#[test]
fn jit_mov_to_debug_register_fault_guards_are_precise_and_noncommitting() {
    let privilege_cases: [(&str, fn(&mut X86_64Vcpu)); 2] = [
        ("protected-cpl3", |vcpu: &mut X86_64Vcpu| {
            vcpu.sregs.cs.selector = 3
        }),
        ("virtual-8086-cs-rpl0", |vcpu: &mut X86_64Vcpu| {
            vcpu.regs.rflags |= RFLAGS_VM
        }),
    ];
    for (name, configure) in privilege_cases {
        let memory = memory_with_code(&[0x0F, 0x23, 0xD0, 0xEB, 0x00, 0xF4]);
        let mut vcpu = test_vcpu(memory);
        configure(&mut vcpu);
        let before = (vcpu.sregs.dr2, vcpu.regs.rflags, vcpu.regs.rax);

        let region = vcpu
            .jit_compile_region()
            .expect("compile privilege-guarded MOV-to-DR region")
            .expect("dynamic privilege must not block admission");
        vcpu.jit_run_region_native(&region);

        assert_eq!(vcpu.regs.rip, 0, "{name}: precise fault PC");
        assert_eq!(
            (vcpu.sregs.dr2, vcpu.regs.rflags, vcpu.regs.rax),
            before,
            "{name}: native guard must not commit"
        );
        assert!(vcpu.step().is_err(), "{name}: direct replay must #GP(0)");
    }

    let memory = memory_with_code(&[0x0F, 0x23, 0xE0, 0xEB, 0x00, 0xF4]);
    let mut de = test_vcpu(memory);
    de.sregs.cr4 |= 1 << 3;
    let before_dr6 = de.sregs.dr6;
    let region = de
        .jit_compile_region()
        .expect("compile DE-guarded MOV-to-DR region")
        .expect("dynamic CR4.DE must not block admission");
    de.jit_run_region_native(&region);
    assert_eq!(de.regs.rip, 0);
    assert_eq!(de.sregs.dr6, before_dr6);
    assert!(de.step().is_err(), "direct replay must #UD for DR4");

    let memory = memory_with_code(&[0x0F, 0x23, 0xC0, 0xEB, 0x00, 0xF4]);
    let mut gd = test_vcpu(memory);
    gd.sregs.dr6 = 0x400;
    gd.sregs.dr7 = 1 << 13;
    let before_dr0 = gd.sregs.dr0;
    let region = gd
        .jit_compile_region()
        .expect("compile GD-guarded MOV-to-DR region")
        .expect("dynamic DR7.GD must not block admission");
    gd.jit_run_region_native(&region);
    assert_eq!(gd.regs.rip, 0);
    assert_eq!(gd.sregs.dr0, before_dr0);
    assert_eq!(gd.sregs.dr6, 0x400, "native guard does not set BD");
    assert_eq!(gd.sregs.dr7, 1 << 13, "native guard does not clear GD");
    assert!(gd.step().is_err(), "direct replay must deliver #DB");
    assert_ne!(gd.sregs.dr6 & (1 << 13), 0, "direct replay sets BD");

    let memory = memory_with_code(&[0x0F, 0x23, 0xF0, 0xEB, 0x00, 0xF4]);
    let mut high = test_vcpu(memory);
    high.regs.rax = 0x0000_0001_0000_0000;
    let before_dr6 = high.sregs.dr6;
    let region = high
        .jit_compile_region()
        .expect("compile high-half-guarded MOV-to-DR region")
        .expect("dynamic source value must not block admission");
    high.jit_run_region_native(&region);
    assert_eq!(high.regs.rip, 0);
    assert_eq!(high.sregs.dr6, before_dr6);
    assert!(high.step().is_err(), "direct replay must #GP(0)");
}

#[test]
fn jit_mov_to_debug_register_new_gd_value_guards_the_next_access() {
    let memory = memory_with_code(&[
        0x0F, 0x23, 0xF8, // mov dr7,rax
        0x0F, 0x23, 0xC3, // mov dr0,rbx
        0xEB, 0x00, 0xF4,
    ]);
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rax = 1 << 13;
    vcpu.regs.rbx = 0x2222;
    let before_dr0 = vcpu.sregs.dr0;
    let region = vcpu
        .jit_compile_region()
        .expect("compile GD-enabling MOV-to-DR sequence")
        .expect("dynamic GD transition must remain native eligible");

    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.regs.rip, 3, "second MOV is the exact frontier");
    assert_eq!(vcpu.sregs.dr7, 1 << 13, "first write committed");
    assert_eq!(vcpu.sregs.dr0, before_dr0, "second write did not commit");
    assert_eq!(vcpu.sregs.dr6, 0x400, "native guard leaves BD to replay");
    assert!(vcpu.step().is_err(), "direct replay must deliver #DB");
    assert_ne!(vcpu.sregs.dr6 & (1 << 13), 0);
}

#[test]
fn jit_rejects_mov_to_debug_register_outside_cs_l() {
    let memory = memory_with_code(&[0x66, 0x0F, 0x23, 0xD0, 0xEB, 0x00, 0xF4]);
    let mut long_mode = test_vcpu(memory.clone());
    assert!(
        long_mode.jit_compile_region().unwrap().is_some(),
        "64-bit MOV-to-DR baseline must compile"
    );

    let mut compatibility = test_vcpu(memory);
    compatibility.sregs.cs.l = false;
    compatibility.sregs.cs.db = true;
    compatibility.regs.rax = 0xFFFF_AAAA_8765_4321;
    assert!(
        compatibility.jit_compile_region().unwrap().is_none(),
        "compatibility-mode MOV-to-DR must remain on the 32-bit direct path"
    );
    assert!(compatibility.step().unwrap().is_none());
    assert_eq!(compatibility.sregs.dr2, 0x8765_4321);
}

#[test]
fn jit_mov_to_debug_register_is_visible_to_a_direct_callout() {
    let memory = memory_with_code(&[
        0x0F, 0x23, 0xD0, // mov dr2,rax
        0xE8, 0xF8, 0x00, 0x00, 0x00, // call 100h
        0xEB, 0x00, // jmp hlt
        0xF4,
    ]);
    // mov rbx,dr2; ret
    memory
        .write_slice(&[0x0F, 0x21, 0xD3, 0xC3], GuestAddress(0x100))
        .unwrap();
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.set_jit_call(true);
        vcpu.regs.rax = 0xCAFE_BABE_1234_5678;
        vcpu.regs.rbx = 0;
    }

    run_direct_to(&mut direct, 10);
    let region = native
        .jit_compile_region()
        .expect("compile MOV-to-DR callout region")
        .expect("MOV-to-DR followed by callout must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.sregs.dr2, direct.sregs.dr2);
    assert_eq!(native.regs.rbx, direct.regs.rbx);
    assert_eq!(native.regs.rbx, 0xCAFE_BABE_1234_5678);
    assert_eq!(native.regs.rsp, direct.regs.rsp);
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 10);
}
