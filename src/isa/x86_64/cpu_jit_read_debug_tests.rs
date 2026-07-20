//! Native x86-64 JIT differentials for MOV-from-debug-register state.

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
    vcpu.sregs.dr0 = 0x1111_2222_3333_4444;
    vcpu.sregs.dr1 = 0x2222_3333_4444_5555;
    vcpu.sregs.dr2 = 0x3333_4444_5555_6666;
    vcpu.sregs.dr3 = 0x4444_5555_6666_7777;
    vcpu.sregs.dr6 = 0xFFFF_0FF0;
    vcpu.sregs.dr7 = 0x400;
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
            vcpu.step().expect("direct MOV-from-DR sequence").is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct execution did not reach {target:#x}");
}

#[test]
fn jit_mov_from_debug_registers_matches_direct_for_every_selector_and_alias() {
    let memory = memory_with_code(&[
        0x0F, 0x21, 0xC0, // mov rax,dr0
        0x0F, 0x21, 0xC9, // mov rcx,dr1
        0x0F, 0x21, 0xD2, // mov rdx,dr2
        0x0F, 0x21, 0xDB, // mov rbx,dr3
        0x0F, 0x21, 0xE6, // mov rsi,dr4 (DR6 alias)
        0x0F, 0x21, 0xEF, // mov rdi,dr5 (DR7 alias)
        0x41, 0x0F, 0x21, 0xF6, // mov r14,dr6
        0x41, 0x0F, 0x21, 0xFF, // mov r15,dr7
        0xEB, 0x00, // jmp hlt
        0xF4,
    ]);
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);

    run_direct_to(&mut direct, 28);
    let region = native
        .jit_compile_region()
        .expect("compile MOV-from-DR region")
        .expect("MOV-from-DR region must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(
        [
            native.regs.rax,
            native.regs.rcx,
            native.regs.rdx,
            native.regs.rbx,
            native.regs.rsi,
            native.regs.rdi,
            native.regs.r14,
            native.regs.r15,
            native.regs.rsp,
            native.regs.rbp,
            native.regs.rflags,
            native.regs.rip,
        ],
        [
            direct.regs.rax,
            direct.regs.rcx,
            direct.regs.rdx,
            direct.regs.rbx,
            direct.regs.rsi,
            direct.regs.rdi,
            direct.regs.r14,
            direct.regs.r15,
            direct.regs.rsp,
            direct.regs.rbp,
            direct.regs.rflags,
            direct.regs.rip,
        ]
    );
}

#[test]
fn jit_mov_from_debug_register_handles_rsp_rbp_destinations() {
    let memory = memory_with_code(&[
        0x0F, 0x21, 0xE4, // mov rsp,dr4 (DR6 alias)
        0x0F, 0x21, 0xED, // mov rbp,dr5 (DR7 alias)
        0xEB, 0x00, 0xF4,
    ]);
    let mut direct = test_vcpu(memory.clone());
    let mut native = test_vcpu(memory);

    run_direct_to(&mut direct, 8);
    let region = native
        .jit_compile_region()
        .expect("compile stack-register MOV-from-DR region")
        .expect("state-backed RSP/RBP destinations must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.regs.rsp, direct.regs.rsp);
    assert_eq!(native.regs.rbp, direct.regs.rbp);
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 8);
}

#[test]
fn jit_verify_snapshots_compares_and_adopts_all_debug_state() {
    let memory = memory_with_code(&[0x0F, 0x21, 0xD8, 0xEB, 0x00, 0xF4]);
    let mut vcpu = test_vcpu(memory);
    let expected_dr3 = vcpu.sregs.dr3;
    let before = (
        vcpu.sregs.dr0,
        vcpu.sregs.dr1,
        vcpu.sregs.dr2,
        vcpu.sregs.dr3,
        vcpu.sregs.dr6,
        vcpu.sregs.dr7,
    );
    let region = vcpu
        .jit_compile_region()
        .expect("compile verified MOV-from-DR region")
        .expect("verified MOV-from-DR region must be native eligible");

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.regs.rax, expected_dr3);
    assert_eq!(
        (
            vcpu.sregs.dr0,
            vcpu.sregs.dr1,
            vcpu.sregs.dr2,
            vcpu.sregs.dr3,
            vcpu.sregs.dr6,
            vcpu.sregs.dr7,
        ),
        before
    );
    assert_eq!(vcpu.regs.rip, 5);
}

#[test]
fn jit_mov_from_debug_register_fault_guards_are_precise_and_noncommitting() {
    let privilege_cases: [(&str, fn(&mut X86_64Vcpu)); 2] = [
        ("protected-cpl3", |vcpu: &mut X86_64Vcpu| {
            vcpu.sregs.cs.selector = 3
        }),
        ("virtual-8086-cs-rpl0", |vcpu: &mut X86_64Vcpu| {
            vcpu.regs.rflags |= RFLAGS_VM
        }),
    ];
    for (name, configure) in privilege_cases {
        let memory = memory_with_code(&[0x0F, 0x21, 0xD0, 0xEB, 0x00, 0xF4]);
        let mut vcpu = test_vcpu(memory);
        configure(&mut vcpu);
        vcpu.regs.rax = 0xA5A5_5A5A_DEAD_BEEF;
        let before = (vcpu.regs.rax, vcpu.regs.rflags, vcpu.sregs.dr2);

        let region = vcpu
            .jit_compile_region()
            .expect("compile privilege-guarded MOV-from-DR region")
            .expect("dynamic privilege must not block admission");
        vcpu.jit_run_region_native(&region);

        assert_eq!(vcpu.regs.rip, 0, "{name}: precise fault PC");
        assert_eq!(
            (vcpu.regs.rax, vcpu.regs.rflags, vcpu.sregs.dr2),
            before,
            "{name}: native guard must not commit"
        );
        assert!(vcpu.step().is_err(), "{name}: direct replay must #GP(0)");
    }

    let memory = memory_with_code(&[0x0F, 0x21, 0xE0, 0xEB, 0x00, 0xF4]);
    let mut de = test_vcpu(memory);
    de.sregs.cr4 |= 1 << 3;
    de.regs.rax = 0xA5A5_5A5A_DEAD_BEEF;
    let region = de
        .jit_compile_region()
        .expect("compile DE-guarded MOV-from-DR region")
        .expect("dynamic CR4.DE must not block admission");
    de.jit_run_region_native(&region);
    assert_eq!(de.regs.rip, 0);
    assert_eq!(de.regs.rax, 0xA5A5_5A5A_DEAD_BEEF);
    assert!(de.step().is_err(), "direct replay must #UD for DR4");

    let memory = memory_with_code(&[0x0F, 0x21, 0xC0, 0xEB, 0x00, 0xF4]);
    let mut gd = test_vcpu(memory);
    gd.sregs.dr6 = 0x400;
    gd.sregs.dr7 = 1 << 13;
    gd.regs.rax = 0xA5A5_5A5A_DEAD_BEEF;
    let region = gd
        .jit_compile_region()
        .expect("compile GD-guarded MOV-from-DR region")
        .expect("dynamic DR7.GD must not block admission");
    gd.jit_run_region_native(&region);
    assert_eq!(gd.regs.rip, 0);
    assert_eq!(gd.regs.rax, 0xA5A5_5A5A_DEAD_BEEF);
    assert_eq!(gd.sregs.dr6, 0x400, "native guard does not set BD");
    assert_eq!(gd.sregs.dr7, 1 << 13, "native guard does not clear GD");
    assert!(gd.step().is_err(), "direct replay must deliver #DB");
    assert_ne!(gd.sregs.dr6 & (1 << 13), 0, "direct replay sets BD");
}

#[test]
fn jit_rejects_mov_from_debug_register_outside_cs_l() {
    let memory = memory_with_code(&[0x66, 0x0F, 0x21, 0xD0, 0xEB, 0x00, 0xF4]);
    let mut long_mode = test_vcpu(memory.clone());
    assert!(
        long_mode.jit_compile_region().unwrap().is_some(),
        "64-bit MOV-from-DR baseline must compile"
    );

    let mut compatibility = test_vcpu(memory);
    compatibility.sregs.cs.l = false;
    compatibility.sregs.cs.db = true;
    compatibility.sregs.dr2 = 0xFFFF_AAAA_8765_4321;
    compatibility.regs.rax = u64::MAX;
    assert!(
        compatibility.jit_compile_region().unwrap().is_none(),
        "compatibility-mode MOV-from-DR must remain on the 32-bit direct path"
    );
    assert!(compatibility.step().unwrap().is_none());
    assert_eq!(compatibility.regs.rax, 0x8765_4321);
}

#[test]
fn jit_mov_from_debug_register_observes_direct_callout_writes() {
    let memory = memory_with_code(&[
        0xE8, 0xFB, 0x00, 0x00, 0x00, // call 100h
        0x0F, 0x21, 0xD3, // mov rbx,dr2
        0xEB, 0x00, // jmp hlt
        0xF4,
    ]);
    // mov dr2,rax; ret
    memory
        .write_slice(&[0x0F, 0x23, 0xD0, 0xC3], GuestAddress(0x100))
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
        .expect("compile debug-register callout region")
        .expect("callout followed by MOV-from-DR must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.sregs.dr2, direct.sregs.dr2);
    assert_eq!(native.regs.rbx, direct.regs.rbx);
    assert_eq!(native.regs.rbx, 0xCAFE_BABE_1234_5678);
    assert_eq!(native.regs.rsp, direct.regs.rsp);
    assert_eq!(native.regs.rflags, direct.regs.rflags);
    assert_eq!(native.regs.rip, 10);
}
