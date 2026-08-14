//! Native x86 `LEAVE` execution, state coherence, and fault-handoff coverage.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const CR0_PE: u64 = 1;
const CR0_AM: u64 = 1 << 18;
const EFER_LMA: u64 = 1 << 10;
const SCANNER_PREFIXES: &[&[u8]] = &[
    &[],
    &[0x66],
    &[0xF2],
    &[0xF3],
    &[0x67],
    &[0x64],
    &[0x65],
    &[0x48],
    &[0x44],
    &[0x41],
    &[0x4D],
    &[0x66, 0x48],
    &[0xF2, 0x48],
    &[0xF3, 0x48],
];

fn memory_with_ranges(code: &[u8], ranges: &[(GuestAddress, usize)]) -> Arc<GuestMemoryMmap> {
    let memory = Arc::new(GuestMemoryMmap::<()>::from_ranges(ranges).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    memory_with_ranges(code, &[(GuestAddress(0), 0x1_0000)])
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.cr0 = CR0_PE;
    vcpu.sregs.efer = EFER_LMA;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.regs.rip = 0;
    vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
    vcpu.regs.rcx = 0x1111_2222_3333_4444;
    vcpu.regs.rdx = 0x5555_6666_7777_8888;
    vcpu.regs.rbx = 0xBBBB_BBBB_BBBB_BBBB;
    vcpu.regs.rsp = 0x9000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rsi = 0x9999_AAAA_BBBB_CCCC;
    vcpu.regs.rdi = 0xDDDD_EEEE_FFFF_0000;
    vcpu.regs.r8 = 0x0808_0808_0808_0808;
    vcpu.regs.r9 = 0x0909_0909_0909_0909;
    vcpu.regs.r10 = 0x1010_1010_1010_1010;
    vcpu.regs.r11 = 0x1111_1111_1111_1111;
    vcpu.regs.r12 = 0x1212_1212_1212_1212;
    vcpu.regs.r13 = 0x1313_1313_1313_1313;
    vcpu.regs.r14 = 0x1414_1414_1414_1414;
    vcpu.regs.r15 = 0x1515_1515_1515_1515;
    vcpu.regs.r31 = 0x3131_3131_3131_3131;
    vcpu.regs.rflags = 0x2 | 0x08D5 | flags::bits::DF;
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);
    vcpu
}

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn assert_scalar_equal(native: &X86_64Vcpu, direct: &X86_64Vcpu, name: &str) {
    assert_eq!(gprs(&native.regs), gprs(&direct.regs), "{name}: GPRs");
    assert_eq!(native.regs.rflags, direct.regs.rflags, "{name}: RFLAGS");
    assert_eq!(native.regs.rip, direct.regs.rip, "{name}: RIP");
}

#[test]
fn native_leave_matches_direct_for_every_scanner_image_and_apx() {
    let mut images = 0usize;
    for prefix in SCANNER_PREFIXES {
        let mut instruction = prefix.to_vec();
        instruction.push(0xC9);
        let width = if prefix.contains(&0x66) && !prefix.iter().any(|byte| byte & 0xF8 == 0x48) {
            2_u8
        } else {
            8
        };
        let mut code = instruction.clone();
        code.push(0xF4);
        let direct_memory = memory_with_code(&code);
        let native_memory = memory_with_code(&code);
        let saved_rbp = 0xA1B2_C3D4_E5F6_BEEF_u64;
        for memory in [&direct_memory, &native_memory] {
            memory
                .write_slice(
                    &saved_rbp.to_le_bytes()[..usize::from(width)],
                    GuestAddress(0x7000),
                )
                .unwrap();
        }
        let mut direct = test_vcpu(direct_memory);
        let mut native = test_vcpu(native_memory);

        assert!(
            direct
                .step()
                .unwrap_or_else(|error| panic!("{instruction:02X?}: direct: {error:#}"))
                .is_none()
        );
        let region = native
            .jit_compile_region()
            .unwrap_or_else(|error| panic!("{instruction:02X?}: compile: {error:#}"))
            .unwrap_or_else(|| panic!("{instruction:02X?}: not native eligible"));
        native.jit_run_region_native(&region);

        assert_scalar_equal(&native, &direct, &format!("{instruction:02X?}"));
        assert_eq!(native.regs.rsp, 0x7000 + u64::from(width));
        images += 1;
    }

    let instruction = [0xD5, 0x00, 0xC9];
    let mut code = instruction.to_vec();
    code.push(0xF4);
    let direct_memory = memory_with_code(&code);
    let native_memory = memory_with_code(&code);
    for memory in [&direct_memory, &native_memory] {
        memory
            .write_obj(0xCAFE_BABE_0123_4567_u64, GuestAddress(0x7000))
            .unwrap();
    }
    let mut direct = test_vcpu(direct_memory);
    let mut native = test_vcpu(native_memory);
    direct.set_apx_enabled(true);
    native.set_apx_enabled(true);
    assert!(direct.step().expect("direct APX LEAVE").is_none());
    let region = native
        .jit_compile_region()
        .expect("compile APX LEAVE")
        .expect("APX LEAVE must be native eligible");
    native.jit_run_region_native(&region);
    assert_scalar_equal(&native, &direct, "APX LEAVE");
    images += 1;

    assert_eq!(images, SCANNER_PREFIXES.len() + 1);
}

#[test]
fn native_rex2_leave_matches_direct_for_every_map0_payload_and_width() {
    let mut images = 0usize;
    for payload in 0_u8..=0x7F {
        let instruction = [0x66, 0xD5, payload, 0xC9];
        let width = if payload & 0x08 != 0 { 8_u8 } else { 2 };
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let direct_memory = memory_with_code(&code);
        let native_memory = memory_with_code(&code);
        let saved_rbp = 0xA1B2_C3D4_E5F6_BEEF_u64;
        for memory in [&direct_memory, &native_memory] {
            memory
                .write_slice(
                    &saved_rbp.to_le_bytes()[..usize::from(width)],
                    GuestAddress(0x7000),
                )
                .unwrap();
        }
        let mut direct = test_vcpu(direct_memory);
        let mut native = test_vcpu(native_memory);
        direct.set_apx_enabled(true);
        native.set_apx_enabled(true);

        assert!(
            direct
                .step()
                .unwrap_or_else(|error| panic!("{instruction:02X?}: direct: {error:#}"))
                .is_none()
        );
        let region = native
            .jit_compile_region()
            .unwrap_or_else(|error| panic!("{instruction:02X?}: compile: {error:#}"))
            .unwrap_or_else(|| panic!("{instruction:02X?}: not native eligible"));
        native.jit_run_region_native(&region);

        assert_scalar_equal(&native, &direct, &format!("{instruction:02X?}"));
        assert_eq!(native.regs.rsp, 0x7000 + u64::from(width));
        images += 1;
    }
    assert_eq!(images, 128);
}

#[test]
fn native_leave16_sparse_frame_deopts_then_direct_replay_uses_full_address() {
    const FRAME: u64 = 0x0000_0001_0000_7000;
    let code = [0x66, 0xC9, 0xF4];
    let ranges = [(GuestAddress(0), 0x1000), (GuestAddress(FRAME), 0x1000)];
    let direct_memory = memory_with_ranges(&code, &ranges);
    let native_memory = memory_with_ranges(&code, &ranges);
    direct_memory
        .write_obj(0xBEEF_u16, GuestAddress(FRAME))
        .unwrap();
    native_memory
        .write_obj(0xBEEF_u16, GuestAddress(FRAME))
        .unwrap();
    let mut direct = test_vcpu(direct_memory);
    let mut native = test_vcpu(native_memory);
    direct.regs.rbp = FRAME;
    native.regs.rbp = FRAME;

    assert!(direct.step().expect("direct W16 LEAVE").is_none());
    let region = native
        .jit_compile_region()
        .expect("compile W16 LEAVE")
        .expect("W16 LEAVE must be native eligible");
    native.jit_run_region_native(&region);

    // The scalar helper deliberately recognizes only the contiguous RAM region
    // rooted at guest-physical address 0. This sparse high region therefore
    // deoptimizes before any architectural commit, then exact direct replay
    // owns the single observable read.
    assert_eq!(native.regs.rsp, 0x9000);
    assert_eq!(native.regs.rbp, FRAME);
    assert_eq!(native.regs.rip, 0);
    assert!(native.step().expect("replay sparse W16 LEAVE").is_none());

    assert_scalar_equal(&native, &direct, "high-address W16 LEAVE replay");
    assert_eq!(native.regs.rsp, FRAME + 2);
    assert_eq!(native.regs.rbp, 0x0000_0001_0000_BEEF);
}

#[test]
fn native_leave_state_backed_outputs_feed_successor_scalar_work() {
    let code = [
        0xC9, // leave
        0x48, 0x89, 0xE0, // mov rax,rsp
        0x48, 0x8D, 0x6D, 0x01, // lea rbp,[rbp+1]
        0xF4,
    ];
    let direct_memory = memory_with_code(&code);
    let native_memory = memory_with_code(&code);
    for memory in [&direct_memory, &native_memory] {
        memory
            .write_obj(0x1234_5678_9ABC_DEF0_u64, GuestAddress(0x7000))
            .unwrap();
    }
    let mut direct = test_vcpu(direct_memory);
    let mut native = test_vcpu(native_memory);

    for _ in 0..3 {
        assert!(
            direct
                .step()
                .expect("direct LEAVE successor sequence")
                .is_none()
        );
    }
    let region = native
        .jit_compile_region()
        .expect("compile LEAVE successor sequence")
        .expect("LEAVE successor sequence must be native eligible");
    native.jit_run_region_native(&region);

    assert_scalar_equal(&native, &direct, "LEAVE successor state");
    assert_eq!(native.regs.rax, 0x7008);
    assert_eq!(native.regs.rbp, 0x1234_5678_9ABC_DEF1);
    assert_eq!(native.regs.rip, 8);
}

#[test]
fn native_leave_faults_deopt_without_commit_for_exact_direct_replay() {
    struct Case {
        name: &'static str,
        frame: u64,
        configure: fn(&mut X86_64Vcpu),
        vector: Option<u8>,
    }
    let cases = [
        Case {
            name: "unmapped pop",
            frame: 0x2_0000,
            configure: |_| {},
            vector: None,
        },
        Case {
            name: "noncanonical stack address",
            frame: 0x0000_8000_0000_0000,
            configure: |_| {},
            vector: Some(12),
        },
        Case {
            name: "stack access crosses the canonical boundary",
            frame: 0x0000_7FFF_FFFF_FFFC,
            configure: |_| {},
            vector: Some(12),
        },
        Case {
            name: "unaligned user stack",
            frame: 0x7001,
            configure: |vcpu| {
                vcpu.sregs.cr0 |= CR0_AM;
                vcpu.sregs.cs.selector = 3;
                vcpu.regs.rflags |= flags::bits::AC;
            },
            vector: Some(17),
        },
    ];

    for case in cases {
        let mut vcpu = test_vcpu(memory_with_code(&[0xC9, 0xF4]));
        vcpu.regs.rbp = case.frame;
        (case.configure)(&mut vcpu);
        let before = vcpu.regs.clone();
        let region = vcpu
            .jit_compile_region()
            .unwrap_or_else(|error| panic!("{}: compile: {error:#}", case.name))
            .unwrap_or_else(|| panic!("{}: guarded LEAVE not eligible", case.name));

        vcpu.jit_run_region_native(&region);
        assert_eq!(vcpu.regs.rsp, before.rsp, "{}: unchanged RSP", case.name);
        assert_eq!(vcpu.regs.rbp, case.frame, "{}: unchanged RBP", case.name);
        assert_eq!(vcpu.regs.rip, 0, "{}: precise frontier", case.name);
        assert_eq!(vcpu.regs.rflags, before.rflags, "{}: RFLAGS", case.name);

        let error = format!("{:#}", vcpu.step().expect_err("direct replay must fault"));
        if let Some(vector) = case.vector {
            assert!(
                error.contains(&format!("IDT entry {vector} not present")),
                "{}: {error}",
                case.name
            );
        }
        assert_eq!(vcpu.regs.rsp, before.rsp, "{}: replay RSP", case.name);
        assert_eq!(vcpu.regs.rbp, case.frame, "{}: replay RBP", case.name);
        assert_eq!(vcpu.regs.rip, 0, "{}: replay fault PC", case.name);
    }
}

#[test]
fn native_rex2_leave_guard_precedes_rsp_commit_and_compatibility_is_rejected() {
    let code = [0xD5, 0x00, 0xC9, 0xF4];
    let memory = memory_with_code(&code);
    memory
        .write_obj(0xCAFE_BABE_0123_4567_u64, GuestAddress(0x7000))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    vcpu.set_apx_enabled(true);
    let region = vcpu
        .jit_compile_region()
        .expect("compile dynamically guarded REX2 LEAVE")
        .expect("REX2 LEAVE must remain native eligible");
    vcpu.set_apx_enabled(false);
    let before = vcpu.regs.clone();

    vcpu.jit_run_region_native(&region);
    assert_eq!(gprs(&vcpu.regs), gprs(&before));
    assert_eq!(vcpu.regs.rflags, before.rflags);
    assert_eq!(vcpu.regs.rip, before.rip);
    let error = format!("{:#}", vcpu.step().expect_err("APX-disabled direct replay"));
    assert!(error.contains("IDT entry 6 not present"), "{error}");
    assert_eq!(gprs(&vcpu.regs), gprs(&before));

    let compatibility_memory = memory_with_code(&[0xC9, 0xF4]);
    compatibility_memory
        .write_obj(0x89AB_CDEF_u32, GuestAddress(0x7000))
        .unwrap();
    let mut compatibility = test_vcpu(compatibility_memory.clone());
    compatibility.sregs.cs.l = false;
    compatibility.sregs.cs.db = true;
    compatibility.sregs.ss.db = true;
    assert!(
        compatibility.jit_compile_region().unwrap().is_none(),
        "compatibility-mode LEAVE must remain on the direct path"
    );

    let mut dynamic = test_vcpu(compatibility_memory);
    let region = dynamic
        .jit_compile_region()
        .expect("compile long-mode LEAVE before mode change")
        .expect("long-mode LEAVE must be native eligible");
    dynamic.sregs.cs.l = false;
    dynamic.sregs.cs.db = true;
    dynamic.sregs.ss.db = true;
    let before = dynamic.regs.clone();
    dynamic.jit_run_region_native(&region);
    assert_eq!(gprs(&dynamic.regs), gprs(&before));
    assert_eq!(dynamic.regs.rip, 0);
    assert!(
        dynamic
            .step()
            .expect("direct compatibility LEAVE")
            .is_none()
    );
    assert_eq!(dynamic.regs.rsp, 0x7004);
    assert_eq!(dynamic.regs.rbp, 0x89AB_CDEF);
    assert_eq!(dynamic.regs.rip, 1);

    let lma_memory = memory_with_code(&[0xC9, 0xF4]);
    lma_memory
        .write_obj(0xCAFE_BABE_0123_4567_u64, GuestAddress(0x7000))
        .unwrap();
    let mut lma_disabled = test_vcpu(lma_memory);
    let region = lma_disabled
        .jit_compile_region()
        .expect("compile LEAVE before clearing EFER.LMA")
        .expect("long-mode LEAVE must be native eligible");
    lma_disabled.sregs.efer &= !EFER_LMA;
    let before = lma_disabled.regs.clone();
    lma_disabled.jit_run_region_native(&region);
    assert_eq!(gprs(&lma_disabled.regs), gprs(&before));
    assert_eq!(lma_disabled.regs.rflags, before.rflags);
    assert_eq!(lma_disabled.regs.rip, 0);
}

#[test]
fn verified_leave_restores_and_replays_the_native_stack_read() {
    let memory = memory_with_code(&[0xC9, 0xF4]);
    memory
        .write_obj(0xCAFE_BABE_0123_4567_u64, GuestAddress(0x7000))
        .unwrap();
    let mut vcpu = test_vcpu(memory);
    let region = vcpu
        .jit_compile_region()
        .expect("compile verified LEAVE")
        .expect("LEAVE must be native eligible");

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.regs.rsp, 0x7008);
    assert_eq!(vcpu.regs.rbp, 0xCAFE_BABE_0123_4567);
    assert_eq!(vcpu.regs.rip, 1);
}
