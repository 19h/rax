//! End-to-end M4 integration test: the SMIR native hot-block JIT tier executing
//! through the real `X86_64Vcpu` state, validated against the interpreter.
//!
//! Run with: `cargo test --features smir-jit --test smir_jit_vcpu -- --nocapture`
#![cfg(all(feature = "smir-jit", target_arch = "x86_64"))]

use std::sync::Arc;
use std::time::Instant;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap, GuestRegionMmap, MmapRegion};

use rax::isa::x86_64::X86_64Vcpu;
use rax::vm::vcpu::{Registers, SystemRegisters, VCpu, VcpuExit};

const LOAD_ADDR: u64 = 0x10_0000;
const MEM_SIZE: u64 = 16 * 1024 * 1024;

#[path = "x86_64/ah_flags.rs"]
mod ah_flags;
#[path = "x86_64/amx_disabled.rs"]
mod amx_disabled;
#[path = "x86_64/apx_bmi.rs"]
mod apx_bmi;
#[path = "x86_64/apx_cet.rs"]
mod apx_cet;
#[path = "x86_64/apx_movrs.rs"]
mod apx_movrs;
#[path = "x86_64/apx_nf_reserved.rs"]
mod apx_nf_reserved;
#[path = "x86_64/apx_push2_pop2.rs"]
mod apx_push2_pop2;
#[path = "x86_64/apx_reserved.rs"]
mod apx_reserved;
#[path = "x86_64/cmpccxadd.rs"]
mod cmpccxadd;
#[path = "x86_64/cmpxchg_register.rs"]
mod cmpxchg_register;
#[path = "x86_64/flag_control.rs"]
mod flag_control;
#[path = "x86_64/group3_alias.rs"]
mod group3_alias;
#[path = "x86_64/legacy_0f38_terminal.rs"]
mod legacy_0f38_terminal;
#[path = "x86_64/legacy_0f3a_reserved.rs"]
mod legacy_0f3a_reserved;
#[path = "x86_64/legacy_high_byte.rs"]
mod legacy_high_byte;
#[path = "x86_64/legacy_packed_extend.rs"]
mod legacy_packed_extend;
#[path = "x86_64/legacy_widening_dword_multiply.rs"]
mod legacy_widening_dword_multiply;
#[path = "x86_64/mmx_xmm_transfer.rs"]
mod mmx_xmm_transfer;
#[path = "x86_64/multiply_register.rs"]
mod multiply_register;
#[path = "x86_64/ordinary_stack.rs"]
mod ordinary_stack;
#[path = "x86_64/rdpid.rs"]
mod rdpid;
#[path = "x86_64/smc.rs"]
mod smc;
#[path = "x86_64/sse4a_bitfield.rs"]
mod sse4a_bitfield;
#[path = "x86_64/tbm.rs"]
mod tbm;
#[path = "x86_64/three_dnow_reserved.rs"]
mod three_dnow_reserved;
#[path = "x86_64/vector_legacy_prefix_reserved.rs"]
mod vector_legacy_prefix_reserved;
#[path = "x86_64/vector_prefix_reserved.rs"]
mod vector_prefix_reserved;
#[path = "x86_64/vex_bmi_reserved.rs"]
mod vex_bmi_reserved;
#[path = "x86_64/xchg_register.rs"]
mod xchg_register;

/// Build a vcpu loaded with the `bench_loop` hot loop for `iters` iterations.
//   xor eax,eax ; mov ecx,iters ; loop: add eax,3 ; xor edx,edx ; sub eax,1 ;
//   dec ecx ; jnz loop ; hlt
fn make_vcpu(iters: u32) -> X86_64Vcpu {
    let mut code: Vec<u8> = vec![0x31, 0xC0]; // xor eax,eax
    code.push(0xB9); // mov ecx, imm32
    code.extend_from_slice(&iters.to_le_bytes());
    code.extend_from_slice(&[0x83, 0xC0, 0x03]); // add eax,3
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx,edx
    code.extend_from_slice(&[0x83, 0xE8, 0x01]); // sub eax,1
    code.extend_from_slice(&[0xFF, 0xC9]); // dec ecx
    code.extend_from_slice(&[0x75, 0xF4]); // jnz loop
    code.push(0xF4); // hlt

    let region = MmapRegion::new(MEM_SIZE as usize).unwrap();
    let guest_region = GuestRegionMmap::new(region, GuestAddress(0)).unwrap();
    let memory = Arc::new(GuestMemoryMmap::from_regions(vec![guest_region]).unwrap());
    memory.write_slice(&code, GuestAddress(LOAD_ADDR)).unwrap();

    let mut regs = Registers::default();
    regs.rip = LOAD_ADDR;
    regs.rsp = 0x11_0000;
    regs.rflags = 0x2;

    let mut sregs = SystemRegisters::default();
    sregs.cr0 = 0x21;
    sregs.cr4 = 0x20;
    sregs.efer = 0x500;
    sregs.cs.base = 0;
    sregs.cs.limit = 0xFFFFFFFF;
    sregs.cs.selector = 0x8;
    sregs.cs.type_ = 0xB;
    sregs.cs.present = true;
    sregs.cs.s = true;
    sregs.cs.l = true;
    sregs.cs.g = true;
    sregs.ds.base = 0;
    sregs.ds.limit = 0xFFFFFFFF;
    sregs.ds.selector = 0x10;
    sregs.ds.type_ = 0x3;
    sregs.ds.present = true;
    sregs.ds.db = true;
    sregs.ds.s = true;
    sregs.ds.g = true;
    sregs.es = sregs.ds.clone();
    sregs.fs = sregs.ds.clone();
    sregs.gs = sregs.ds.clone();
    sregs.ss = sregs.ds.clone();

    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.set_regs(&regs).unwrap();
    vcpu.set_sregs(&sregs).unwrap();
    vcpu
}

/// Build a vcpu loaded with arbitrary guest `code` at LOAD_ADDR.
fn make_vcpu_code(code: &[u8]) -> X86_64Vcpu {
    let region = MmapRegion::new(MEM_SIZE as usize).unwrap();
    let guest_region = GuestRegionMmap::new(region, GuestAddress(0)).unwrap();
    let memory = Arc::new(GuestMemoryMmap::from_regions(vec![guest_region]).unwrap());
    memory.write_slice(code, GuestAddress(LOAD_ADDR)).unwrap();

    let mut regs = Registers::default();
    regs.rip = LOAD_ADDR;
    regs.rsp = 0x11_0000;
    regs.rflags = 0x2;

    let mut sregs = SystemRegisters::default();
    sregs.cr0 = 0x21;
    sregs.cr4 = 0x20;
    sregs.efer = 0x500;
    sregs.cs.limit = 0xFFFFFFFF;
    sregs.cs.selector = 0x8;
    sregs.cs.type_ = 0xB;
    sregs.cs.present = true;
    sregs.cs.s = true;
    sregs.cs.l = true;
    sregs.cs.g = true;
    sregs.ds.limit = 0xFFFFFFFF;
    sregs.ds.selector = 0x10;
    sregs.ds.type_ = 0x3;
    sregs.ds.present = true;
    sregs.ds.db = true;
    sregs.ds.s = true;
    sregs.ds.g = true;
    sregs.es = sregs.ds.clone();
    sregs.fs = sregs.ds.clone();
    sregs.gs = sregs.ds.clone();
    sregs.ss = sregs.ds.clone();

    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.set_regs(&regs).unwrap();
    vcpu.set_sregs(&sregs).unwrap();
    vcpu
}

/// Default call mode must admit cross-function regions, while explicit policy
/// disable and structurally ineligible frontiers still fall back cleanly.
#[test]
fn jit_bails_on_ineligible() {
    // (a) `call $+5 ; hlt`: default lift-through-calls compiles the caller and
    //     transfers to the interpreted callee, whose HLT yields back through
    //     the callout. Explicitly disabling call mode restores frontier bailout.
    let mut v = make_vcpu_code(&[0xE8, 0x00, 0x00, 0x00, 0x00, 0xF4]);
    assert!(
        v.jit_try_block().expect("default call-mode jit_try_block"),
        "cross-function call mode must be enabled by default"
    );
    let mut v = make_vcpu_code(&[0xE8, 0x00, 0x00, 0x00, 0x00, 0xF4]);
    v.set_jit_call(false);
    assert!(
        !v.jit_try_block().expect("disabled call-mode jit_try_block"),
        "explicit call-mode disable must restore call-as-frontier fallback"
    );

    // (b) A supported straight-line prefix ending in RET must run natively and
    //     hand off at the exact RET PC; a bare RET still has no native work.
    let mut v = make_vcpu_code(&[0xB8, 0x05, 0x00, 0x00, 0x00, 0xC3]);
    assert!(
        v.jit_try_block().expect("prefixed RET jit_try_block"),
        "the supported prefix before RET must enter the native tier"
    );
    assert_eq!(v.get_regs().unwrap().rax, 5);
    assert_eq!(v.get_regs().unwrap().rip, LOAD_ADDR + 5);

    let mut v = make_vcpu_code(&[0xC3]);
    assert!(
        !v.jit_try_block().expect("bare RET jit_try_block"),
        "a bare entry frontier must still bail"
    );

    // (c) A frontier-less spin loop `jmp $` (EB FE) — running it natively would
    //     loop forever with no way back; must bail (and must NOT hang).
    let mut v = make_vcpu_code(&[0xEB, 0xFE]);
    assert!(
        !v.jit_try_block().expect("jit_try_block"),
        "a frontier-less infinite loop must bail (no native exit)"
    );

    // (d) A loop whose body reads guest memory through the MMU helper path is
    //     native by default. Explicit policy disable retains interpreter fallback.
    let mut v = make_vcpu_code(&[0x8B, 0x03, 0xFF, 0xC9, 0x75, 0xFA, 0xF4]);
    let mut r = v.get_regs().unwrap();
    r.rcx = 5;
    r.rbx = LOAD_ADDR;
    v.set_regs(&r).unwrap();
    assert!(
        v.jit_try_block().expect("default memory jit_try_block"),
        "memory-touching loops must use MMU-helper JIT by default"
    );
    assert_eq!(v.get_regs().unwrap().rcx & 0xffff_ffff, 0);

    let mut v = make_vcpu_code(&[0x8B, 0x03, 0xFF, 0xC9, 0x75, 0xFA, 0xF4]);
    let mut r = v.get_regs().unwrap();
    r.rcx = 5;
    r.rbx = LOAD_ADDR;
    v.set_regs(&r).unwrap();
    v.set_jit_mem(false);
    assert!(
        !v.jit_try_block().expect("disabled memory jit_try_block"),
        "explicit memory-JIT disable must retain interpreter fallback"
    );
}

#[test]
fn jit_executes_supported_prefix_before_senduipi_trap_frontier() {
    // MOV EAX,0x12345678; SENDUIPI. UINTR is unavailable in the configured
    // interpreter, so SENDUIPI is an explicit #UD trap and remains a precise
    // interpreter frontier without discarding the liftable/native prefix.
    let code = [0xB8, 0x78, 0x56, 0x34, 0x12, 0xF3, 0x0F, 0xC7, 0xF0];
    let mut vcpu = make_vcpu_code(&code);
    let before_flags = vcpu.get_regs().unwrap().rflags;
    vcpu.set_mem_recording(true);

    assert!(
        vcpu.jit_try_block()
            .expect("partial region before SENDUIPI trap frontier"),
        "a later trap must not reject the supported prefix"
    );

    let regs = vcpu.get_regs().unwrap();
    assert_eq!(regs.rax, 0x1234_5678);
    assert_eq!(regs.rflags, before_flags);
    assert_eq!(
        regs.rip,
        LOAD_ADDR + 5,
        "native exit must point at the unexecuted SENDUIPI"
    );
    let mut memory_records = Vec::new();
    vcpu.drain_mem_records(&mut memory_records);
    assert!(
        memory_records.is_empty(),
        "JIT compilation lookahead must not appear as retired guest memory: {memory_records:?}"
    );
}

#[test]
fn jit_uses_readable_prefix_when_fixed_window_crosses_unmapped_boundary() {
    // Place the same sequence at the final mapped bytes. A fixed 512-byte
    // snapshot crosses the region boundary; prefix probing must retain the nine
    // readable bytes and the interpreter frontier must stop before SENDUIPI.
    let code = [0xB8, 0xBE, 0xBA, 0xFE, 0xCA, 0xF3, 0x0F, 0xC7, 0xF0];
    let (mut vcpu, memory) = make_vcpu_mem(&[]);
    let entry = MEM_SIZE - code.len() as u64;
    memory.write_slice(&code, GuestAddress(entry)).unwrap();
    let mut regs = vcpu.get_regs().unwrap();
    regs.rip = entry;
    vcpu.set_regs(&regs).unwrap();

    assert!(
        vcpu.jit_try_block().expect("partial readable JIT window"),
        "an unmapped lookahead suffix must not reject readable native work"
    );

    let regs = vcpu.get_regs().unwrap();
    assert_eq!(regs.rax, 0xCAFE_BABE);
    assert_eq!(regs.rip, entry + 5);
}

fn run_interp(vcpu: &mut X86_64Vcpu) {
    loop {
        match vcpu.step() {
            Ok(Some(VcpuExit::Hlt)) => break,
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(e) => panic!("interp error: {e:?}"),
        }
    }
}

#[test]
fn interp_lea_scaled_index_wraps() {
    // lea rdx,[rsi*8]; hlt
    let mut v = make_vcpu_code(&[0x48, 0x8D, 0x14, 0xF5, 0x00, 0x00, 0x00, 0x00, 0xF4]);
    let mut r = v.get_regs().unwrap();
    r.rsi = 0x8000_0000_0000_0000;
    v.set_regs(&r).unwrap();

    run_interp(&mut v);

    let r = v.get_regs().unwrap();
    assert_eq!(r.rdx, 0x8000_0000_0000_0000u64.wrapping_mul(8));
}

/// The JIT-tier final state must equal the interpreter's, register for register.
#[test]
fn jit_matches_interpreter() {
    let iters = 1000u32;

    let mut jit = make_vcpu(iters);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "the loop region should JIT and advance to its exit"
    );
    // The JIT ran the loop natively and parked RIP at the HLT (the frontier
    // exit, not yet executed). Step it so both vcpus end at the same point.
    run_interp(&mut jit);
    let jr = jit.get_regs().unwrap();

    let mut interp = make_vcpu(iters);
    run_interp(&mut interp);
    let ir = interp.get_regs().unwrap();

    // Whole-loop native execution must reproduce the interpreter's GPR state.
    assert_eq!(jr.rax, ir.rax, "rax");
    assert_eq!(jr.rcx, ir.rcx, "rcx");
    assert_eq!(jr.rdx, ir.rdx, "rdx");
    assert_eq!(jr.rbx, ir.rbx, "rbx");
    assert_eq!(jr.rsi, ir.rsi, "rsi");
    assert_eq!(jr.rdi, ir.rdi, "rdi");
    assert_eq!(jr.r8, ir.r8, "r8");
    assert_eq!(jr.r15, ir.r15, "r15");
    // Sanity vs the closed-form result: eax = 2*iters, ecx = 0.
    assert_eq!(jr.rax & 0xffff_ffff, (2 * iters as u64) & 0xffff_ffff);
    assert_eq!(jr.rcx & 0xffff_ffff, 0);
}

/// Register-only LEA (address arithmetic, no dereference) + BSF (bit-scan) in a
/// hot loop must JIT bit-exactly vs the interpreter — verifies the whitelist
/// additions (Lea/Bsf/Bsr) lower correctly under the native runtime.
#[test]
fn jit_lea_sib_matches_interpreter() {
    // xor eax,eax; mov ecx,300
    // loop: lea edx,[rax+rax*2+5]  (SIB base+index*scale+disp); add eax,1;
    //       dec ecx; jnz loop; hlt
    let code: &[u8] = &[
        0x31, 0xC0, // xor eax,eax
        0xB9, 0x2C, 0x01, 0x00, 0x00, // mov ecx,300
        0x8D, 0x54, 0x40, 0x05, // loop: lea edx,[rax+rax*2+5]
        0x83, 0xC0, 0x01, // add eax,1
        0xFF, 0xC9, // dec ecx
        0x75, 0xF5, // jnz loop
        0xF4, // hlt
    ];

    let mut jit = make_vcpu_code(code);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "LEA-SIB loop should JIT and advance to its exit"
    );
    run_interp(&mut jit);
    let jr = jit.get_regs().unwrap();

    let mut interp = make_vcpu_code(code);
    run_interp(&mut interp);
    let ir = interp.get_regs().unwrap();

    assert_eq!(jr.rax, ir.rax, "rax");
    assert_eq!(jr.rcx, ir.rcx, "rcx");
    assert_eq!(jr.rdx, ir.rdx, "rdx (lea base+index*scale+disp)");
    assert_eq!(jr.rax & 0xffff_ffff, 300, "closed form: eax == iters");
    // Last iter rax==299: edx = 299*3 + 5 = 902.
    assert_eq!(jr.rdx & 0xffff_ffff, 902, "lea result of last iteration");
}

/// RIP-relative LEA computes a numeric guest virtual address; it neither reads
/// memory nor depends on where the native code buffer was allocated.
#[test]
fn jit_rip_relative_lea_materializes_guest_address_without_memory_helpers() {
    // lea rax,[rip+0x1234]; hlt
    let code = [0x48, 0x8D, 0x05, 0x34, 0x12, 0x00, 0x00, 0xF4];
    let expected = (LOAD_ADDR + 7).wrapping_add_signed(0x1234);

    let mut interp = make_vcpu_code(&code);
    run_interp(&mut interp);
    assert_eq!(interp.get_regs().unwrap().rax, expected);

    let mut jit = make_vcpu_code(&code);
    jit.set_jit_call(false);
    jit.set_jit_mem(false);
    assert!(
        jit.jit_try_block().expect("RIP-relative LEA JIT"),
        "register-only RIP-relative LEA must not be rejected as a relocation"
    );
    let handoff = jit.get_regs().unwrap();
    assert_eq!(handoff.rax, expected);
    assert_eq!(handoff.rip, LOAD_ADDR + 7, "HLT must remain a frontier");
    run_interp(&mut jit);
    assert_eq!(jit.get_regs().unwrap().rax, interp.get_regs().unwrap().rax);
}

/// Guest RSP/RBP are state-backed while native code retains the host stack and
/// frame pointers. Register-only MOV forms must therefore remain JIT-eligible
/// without ever loading guest RSP into the hardware stack pointer.
#[test]
fn jit_state_backed_rsp_rbp_moves_match_interpreter_without_memory_helpers() {
    // Snapshot every source width into r8-r11, then exercise full and partial
    // writes to both state-backed stack registers before HLT.
    let code = [
        0x41, 0x88, 0xE0, // mov r8b,spl
        0x66, 0x41, 0x89, 0xE9, // mov r9w,bp
        0x41, 0x89, 0xE2, // mov r10d,esp
        0x49, 0x89, 0xEB, // mov r11,rbp
        0x48, 0x89, 0xE0, // mov rax,rsp
        0x48, 0xBC, 0xF0, 0xDE, 0xBC, 0x9A, 0x78, 0x56, 0x34, 0x12, // mov rsp,imm64
        0x48, 0x89, 0xE5, // mov rbp,rsp
        0x48, 0x89, 0xE9, // mov rcx,rbp
        0x89, 0xF4, // mov esp,esi
        0x66, 0x89, 0xD4, // mov sp,dx
        0x40, 0x88, 0xFD, // mov bpl,dil
        0xBC, 0x44, 0x33, 0x22, 0x11, // mov esp,0x11223344
        0x66, 0xBC, 0x66, 0x55, // mov sp,0x5566
        0x40, 0xB4, 0x77, // mov spl,0x77
        0xBD, 0xDD, 0xCC, 0xBB, 0xAA, // mov ebp,0xaabbccdd
        0x66, 0xBD, 0xFF, 0xEE, // mov bp,0xeeff
        0x40, 0xB5, 0x11, // mov bpl,0x11
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rdx = 0xA55A;
        regs.rsp = 0x0FED_CBA9_8765_4321;
        regs.rbp = 0xDEAD_BEEF_CAFE_BABE;
        regs.rsi = 0x8765_4321;
        regs.rdi = 0x7B;
        regs.rflags = 0x8D5;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interp = make_vcpu_code(&code);
    setup(&mut interp);
    run_interp(&mut interp);
    let expected = interp.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    jit.set_jit_call(false);
    jit.set_jit_mem(false);
    assert!(
        jit.jit_try_block().expect("state-backed RSP/RBP MOV JIT"),
        "liftable stack-register MOV sequence must enter the native tier"
    );
    assert_eq!(
        jit.get_regs().unwrap().rip,
        LOAD_ADDR + code.len() as u64 - 1
    );
    run_interp(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_eq!(actual.rax, expected.rax, "MOV RAX,RSP");
    assert_eq!(actual.rcx, expected.rcx, "MOV RCX,RBP");
    assert_eq!(actual.r8, expected.r8, "MOV R8B,SPL");
    assert_eq!(actual.r9, expected.r9, "MOV R9W,BP");
    assert_eq!(actual.r10, expected.r10, "MOV R10D,ESP");
    assert_eq!(actual.r11, expected.r11, "MOV R11,RBP");
    assert_eq!(actual.rsp, expected.rsp, "MOV RSP partial/immediate merges");
    assert_eq!(actual.rbp, expected.rbp, "MOV RBP partial/immediate merges");
    assert_eq!(actual.rflags, expected.rflags, "MOV flags");
}

/// Register-source MOVZX/MOVSX/MOVSXD forms use the canonical GuestRegs file
/// whenever either operand is guest RSP/RBP or an APX EGPR. This preserves the
/// host stack/frame pointers, partial destination writes, aliases, and flags.
#[test]
fn jit_state_backed_gpr_extensions_match_interpreter_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        destination_index: usize,
        expected_destination: u64,
    }

    let cases = [
        Case {
            name: "MOVZX SP,BL partial destination",
            instruction: &[0x66, 0x0F, 0xB6, 0xE3],
            apx: false,
            destination_index: 4,
            expected_destination: 0x1234_5678_9ABC_00A7,
        },
        Case {
            name: "MOVSX EBP,BX zeroes upper dword",
            instruction: &[0x0F, 0xBF, 0xEB],
            apx: false,
            destination_index: 5,
            expected_destination: 0x0000_0000_FFFF_80A7,
        },
        Case {
            name: "MOVZX RAX,SPL REX byte source",
            instruction: &[0x48, 0x0F, 0xB6, 0xC4],
            apx: false,
            destination_index: 0,
            expected_destination: 0xF2,
        },
        Case {
            name: "MOVSX RCX,BP state-backed source",
            instruction: &[0x48, 0x0F, 0xBF, 0xCD],
            apx: false,
            destination_index: 1,
            expected_destination: 0xFFFF_FFFF_FFFF_8001,
        },
        Case {
            name: "MOVZX SP,AH legacy high byte",
            instruction: &[0x66, 0x0F, 0xB6, 0xE4],
            apx: false,
            destination_index: 4,
            expected_destination: 0x1234_5678_9ABC_00CD,
        },
        Case {
            name: "MOVSX EBP,BH legacy high byte",
            instruction: &[0x0F, 0xBE, 0xEF],
            apx: false,
            destination_index: 5,
            expected_destination: 0x0000_0000_FFFF_FF80,
        },
        Case {
            name: "MOVSXD RBP,ESP state-backed source and destination",
            instruction: &[0x48, 0x63, 0xEC],
            apx: false,
            destination_index: 5,
            expected_destination: 0xFFFF_FFFF_9ABC_80F2,
        },
        Case {
            name: "MOVZX SP,SPL state-backed alias",
            instruction: &[0x66, 0x40, 0x0F, 0xB6, 0xE4],
            apx: false,
            destination_index: 4,
            expected_destination: 0x1234_5678_9ABC_00F2,
        },
        Case {
            name: "REX2 MOVZX R16,BL state-backed EGPR destination",
            instruction: &[0xD5, 0xC8, 0xB6, 0xC3],
            apx: true,
            destination_index: 16,
            expected_destination: 0xA7,
        },
        Case {
            name: "REX2 MOVSX RAX,R16B state-backed EGPR source",
            instruction: &[0xD5, 0x98, 0xBE, 0xC0],
            apx: true,
            destination_index: 0,
            expected_destination: 0xFFFF_FFFF_FFFF_FFF1,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let setup = |vcpu: &mut X86_64Vcpu, apx: bool| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0123_4567_89AB_CDEF;
        regs.rcx = 0x1111_2222_3333_4444;
        regs.rdx = 0xA5A5_5A5A_1357_2468;
        regs.rbx = 0xFEDC_BA98_7654_80A7;
        regs.rsp = 0x1234_5678_9ABC_80F2;
        regs.rbp = 0x0FED_CBA9_8765_8001;
        regs.rsi = 0x99AA_BBCC_DDEE_FF00;
        regs.rdi = 0x0F1E_2D3C_4B5A_6978;
        regs.r8 = 0x0102_0304_0506_0708;
        regs.r9 = 0x1112_1314_1516_1718;
        regs.r16 = 0xA1A2_A3A4_A5A6_80F1;
        regs.r17 = 0xB1B2_B3B4_B5B6_B7B8;
        regs.r31 = 0xF1F2_F3F4_F5F6_F7F8;
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
        vcpu.set_apx_enabled(apx);
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, case.apx);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        assert_eq!(
            gprs(&expected)[case.destination_index],
            case.expected_destination,
            "{} reference destination",
            case.name
        );

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, case.apx);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// Register-source CMOVcc uses the canonical GuestRegs file whenever either
/// operand is guest RSP/RBP or an APX EGPR. The snapshot must precede the
/// condition evaluation without changing flags, and false-path width semantics
/// must match the architectural interpreter.
#[test]
fn jit_state_backed_gpr_cmov_matches_interpreter_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        rflags: u64,
        destination_index: usize,
        expected_destination: u64,
    }

    const ZF_SET: u64 = 0x2 | 0x8D5;
    const ZF_CLEAR: u64 = 0x2 | (0x8D5 & !(1 << 6));
    let cases = [
        Case {
            name: "CMOVNE SP,BX true partial destination",
            instruction: &[0x66, 0x0F, 0x45, 0xE3],
            apx: false,
            rflags: ZF_CLEAR,
            destination_index: 4,
            expected_destination: 0x1234_5678_9ABC_80A7,
        },
        Case {
            name: "CMOVNE SP,BX false preserves destination",
            instruction: &[0x66, 0x0F, 0x45, 0xE3],
            apx: false,
            rflags: ZF_SET,
            destination_index: 4,
            expected_destination: 0x1234_5678_9ABC_80F2,
        },
        Case {
            name: "CMOVE EBP,ESP true zeroes upper dword",
            instruction: &[0x0F, 0x44, 0xEC],
            apx: false,
            rflags: ZF_SET,
            destination_index: 5,
            expected_destination: 0x9ABC_80F2,
        },
        Case {
            name: "CMOVE EBP,ESP false zeroes upper dword",
            instruction: &[0x0F, 0x44, 0xEC],
            apx: false,
            rflags: ZF_CLEAR,
            destination_index: 5,
            expected_destination: 0x8765_8001,
        },
        Case {
            name: "CMOVS RAX,RBP state-backed source",
            instruction: &[0x48, 0x0F, 0x48, 0xC5],
            apx: false,
            rflags: ZF_SET,
            destination_index: 0,
            expected_destination: 0x0FED_CBA9_8765_8001,
        },
        Case {
            name: "REX2 CMOVNE R16,RBX state-backed EGPR destination",
            instruction: &[0xD5, 0xC8, 0x45, 0xC3],
            apx: true,
            rflags: ZF_CLEAR,
            destination_index: 16,
            expected_destination: 0xFEDC_BA98_7654_80A7,
        },
        Case {
            name: "REX2 CMOVNE RAX,R16 state-backed EGPR source",
            instruction: &[0xD5, 0x98, 0x45, 0xC0],
            apx: true,
            rflags: ZF_CLEAR,
            destination_index: 0,
            expected_destination: 0xA1A2_A3A4_A5A6_80F1,
        },
        Case {
            name: "CMOVNE SP,SP state-backed alias",
            instruction: &[0x66, 0x0F, 0x45, 0xE4],
            apx: false,
            rflags: ZF_CLEAR,
            destination_index: 4,
            expected_destination: 0x1234_5678_9ABC_80F2,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let setup = |vcpu: &mut X86_64Vcpu, case: &Case| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0123_4567_89AB_CDEF;
        regs.rcx = 0x1111_2222_3333_4444;
        regs.rdx = 0xA5A5_5A5A_1357_2468;
        regs.rbx = 0xFEDC_BA98_7654_80A7;
        regs.rsp = 0x1234_5678_9ABC_80F2;
        regs.rbp = 0x0FED_CBA9_8765_8001;
        regs.rsi = 0x99AA_BBCC_DDEE_FF00;
        regs.rdi = 0x0F1E_2D3C_4B5A_6978;
        regs.r8 = 0x0102_0304_0506_0708;
        regs.r9 = 0x1112_1314_1516_1718;
        regs.r16 = 0xA1A2_A3A4_A5A6_80F1;
        regs.r17 = 0xB1B2_B3B4_B5B6_B7B8;
        regs.r31 = 0xF1F2_F3F4_F5F6_F7F8;
        regs.rflags = case.rflags;
        vcpu.set_regs(&regs).unwrap();
        vcpu.set_apx_enabled(case.apx);
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, &case);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        assert_eq!(
            gprs(&expected)[case.destination_index],
            case.expected_destination,
            "{} reference destination",
            case.name
        );

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, &case);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// Standard SETcc performs a byte merge, while APX SETZUcc writes a complete
/// zero-extended GPR. State-backed RSP/RBP and EGPR destinations must preserve
/// those distinct semantics without exposing the host stack or frame pointer.
#[test]
fn jit_state_backed_gpr_setcc_matches_interpreter_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        rflags: u64,
        destination_index: usize,
        expected_destination: u64,
    }

    const ALL_SET: u64 = 0x2 | 0x8D5;
    const ZF_CLEAR: u64 = ALL_SET & !(1 << 6);
    const OF_CLEAR: u64 = ALL_SET & !(1 << 11);
    let cases = [
        Case {
            name: "SETNE SPL true partial destination",
            instruction: &[0x40, 0x0F, 0x95, 0xC4],
            apx: false,
            rflags: ZF_CLEAR,
            destination_index: 4,
            expected_destination: 0x1234_5678_9ABC_8001,
        },
        Case {
            name: "SETNE SPL false partial destination",
            instruction: &[0x40, 0x0F, 0x95, 0xC4],
            apx: false,
            rflags: ALL_SET,
            destination_index: 4,
            expected_destination: 0x1234_5678_9ABC_8000,
        },
        Case {
            name: "SETE BPL true partial destination",
            instruction: &[0x40, 0x0F, 0x94, 0xC5],
            apx: false,
            rflags: ALL_SET,
            destination_index: 5,
            expected_destination: 0x0FED_CBA9_8765_8001,
        },
        Case {
            name: "REX2 SETNE R16B true state-backed EGPR destination",
            instruction: &[0xD5, 0x90, 0x95, 0xC0],
            apx: true,
            rflags: ZF_CLEAR,
            destination_index: 16,
            expected_destination: 0xA1A2_A3A4_A5A6_8001,
        },
        Case {
            name: "APX SETZUO R16 true full destination",
            instruction: &[0x62, 0xFC, 0x7F, 0x18, 0x40, 0xC0],
            apx: true,
            rflags: ALL_SET,
            destination_index: 16,
            expected_destination: 1,
        },
        Case {
            name: "APX SETZUO R16 false full destination",
            instruction: &[0x62, 0xFC, 0x7F, 0x18, 0x40, 0xC0],
            apx: true,
            rflags: OF_CLEAR,
            destination_index: 16,
            expected_destination: 0,
        },
        Case {
            name: "APX SETZUO SPL false full destination",
            instruction: &[0x62, 0xF4, 0x7F, 0x18, 0x40, 0xC4],
            apx: true,
            rflags: OF_CLEAR,
            destination_index: 4,
            expected_destination: 0,
        },
        Case {
            name: "APX SETZUNE BPL true full destination",
            instruction: &[0x62, 0xF4, 0x7F, 0x18, 0x45, 0xC5],
            apx: true,
            rflags: ZF_CLEAR,
            destination_index: 5,
            expected_destination: 1,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let setup = |vcpu: &mut X86_64Vcpu, case: &Case| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0123_4567_89AB_CDEF;
        regs.rcx = 0x1111_2222_3333_4444;
        regs.rdx = 0xA5A5_5A5A_1357_2468;
        regs.rbx = 0xFEDC_BA98_7654_80A7;
        regs.rsp = 0x1234_5678_9ABC_80F2;
        regs.rbp = 0x0FED_CBA9_8765_8001;
        regs.rsi = 0x99AA_BBCC_DDEE_FF00;
        regs.rdi = 0x0F1E_2D3C_4B5A_6978;
        regs.r8 = 0x0102_0304_0506_0708;
        regs.r9 = 0x1112_1314_1516_1718;
        regs.r16 = 0xA1A2_A3A4_A5A6_80F1;
        regs.r17 = 0xB1B2_B3B4_B5B6_B7B8;
        regs.r31 = 0xF1F2_F3F4_F5F6_F7F8;
        regs.rflags = case.rflags;
        vcpu.set_regs(&regs).unwrap();
        vcpu.set_apx_enabled(case.apx);
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, &case);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        assert_eq!(
            gprs(&expected)[case.destination_index],
            case.expected_destination,
            "{} reference destination",
            case.name
        );

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, &case);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

#[test]
fn jit_state_backed_rsp_rbp_add_sub_match_interpreter_without_memory_helpers() {
    // Exercise 8/16/32/64-bit arithmetic with stack registers in destination
    // and source positions. The host stack/frame pointers must never be used as
    // architectural inputs even though the generated arithmetic is native x86.
    let code = [
        0x40, 0x00, 0xC4, // add spl,al
        0x66, 0x29, 0xD5, // sub bp,dx
        0x44, 0x03, 0xC4, // add r8d,esp
        0x4C, 0x29, 0xCD, // sub rbp,r9
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x20;
        regs.rdx = 0x31;
        regs.rsp = 0x1111_2222_3333_44F0;
        regs.rbp = 0xAAAA_BBBB_CCCC_DD10;
        regs.r8 = 0xFFFF_FFFF_0123_4567;
        regs.r9 = 0x0102_0304_0506_0708;
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interp = make_vcpu_code(&code);
    setup(&mut interp);
    run_interp(&mut interp);
    let expected = interp.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    jit.set_jit_call(false);
    jit.set_jit_mem(false);
    assert!(
        jit.jit_try_block()
            .expect("state-backed stack arithmetic JIT"),
        "register-only RSP/RBP ADD/SUB must enter the native tier"
    );
    run_interp(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_eq!(actual.rsp, expected.rsp, "ADD SPL,AL");
    assert_eq!(actual.rbp, expected.rbp, "SUB BP,DX / SUB RBP,R9");
    assert_eq!(actual.r8, expected.r8, "ADD R8D,ESP");
    assert_eq!(actual.rflags, expected.rflags, "arithmetic status flags");
}

#[test]
fn jit_helper_backed_push_pop_is_precise_and_uses_predecrement_rsp() {
    // PUSH RSP; POP R8; PUSH RBP/RAX/imm8; POP R9/R10/R11;
    // PUSH AX; POP BX; HLT.
    let code = [
        0x54, 0x41, 0x58, 0x55, 0x50, 0x6A, 0xFF, 0x41, 0x59, 0x41, 0x5A, 0x41, 0x5B, 0x66, 0x50,
        0x66, 0x5B, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0123_4567_89AB_CDEF;
        regs.rbx = 0xFEDC_BA98_7654_3210;
        regs.rbp = 0x8877_6655_4433_2211;
        regs.rsp = 0x11_0000;
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interp = make_vcpu_code(&code);
    setup(&mut interp);
    run_interp(&mut interp);
    let expected = interp.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    jit.set_jit_mem(true);
    assert!(
        jit.jit_try_block().expect("helper-backed PUSH/POP JIT"),
        "ordinary and RSP-source PUSH/POP sequences must enter the native tier"
    );
    run_interp(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_eq!(actual.r8, 0x11_0000, "PUSH RSP uses pre-decrement value");
    assert_eq!(actual.r9, expected.r9, "sign-extended PUSH imm8");
    assert_eq!(actual.r10, expected.r10, "PUSH/POP RAX");
    assert_eq!(actual.r11, expected.r11, "PUSH/POP RBP");
    assert_eq!(actual.rbx, expected.rbx, "16-bit PUSH AX / POP BX");
    assert_eq!(actual.rsp, expected.rsp, "balanced stack pointer");
    assert_eq!(
        actual.rflags, expected.rflags,
        "stack operations preserve flags"
    );

    // A helper fault must occur before the state-backed SUB commits RSP.
    let mut fault = make_vcpu_code(&[0x50, 0xB9, 0x01, 0x00, 0x00, 0x00, 0xF4]);
    let mut before = fault.get_regs().unwrap();
    before.rax = 0xA5A5_5A5A_1234_5678;
    before.rcx = 0xDEAD_BEEF;
    before.rsp = MEM_SIZE + 4;
    before.rflags = 0x2 | 0x8D5;
    fault.set_regs(&before).unwrap();
    fault.set_jit_mem(true);
    assert!(
        fault.jit_try_block().expect("faulting PUSH JIT"),
        "faulting PUSH must compile before taking the guest-memory fault"
    );
    let after = fault.get_regs().unwrap();
    assert_eq!(after.rip, LOAD_ADDR, "restart at faulting PUSH");
    assert_eq!(after.rsp, before.rsp, "fault must not commit RSP decrement");
    assert_eq!(after.rax, before.rax, "PUSH source survives fault");
    assert_eq!(after.rcx, before.rcx, "post-PUSH instruction must not run");
    assert_eq!(after.rflags, before.rflags, "fault path flags");
}

#[test]
fn jit_helper_backed_pop_aliases_match_interpreter_and_fault_precisely() {
    let run_case = |name: &str, code: &[u8], setup: fn(&mut Registers)| {
        let mut interp = make_vcpu_code(code);
        let mut initial = interp.get_regs().unwrap();
        setup(&mut initial);
        interp.set_regs(&initial).unwrap();
        run_interp(&mut interp);
        let expected = interp.get_regs().unwrap();

        let mut jit = make_vcpu_code(code);
        jit.set_regs(&initial).unwrap();
        jit.set_jit_mem(true);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: POP JIT: {error}")),
            "{name}: exact POP sequence must enter the native tier"
        );
        run_interp(&mut jit);
        let actual = jit.get_regs().unwrap();
        assert_eq!(actual.rsp, expected.rsp, "{name}: RSP");
        assert_eq!(actual.rbp, expected.rbp, "{name}: RBP");
        assert_eq!(actual.rax, expected.rax, "{name}: RAX");
        assert_eq!(actual.rcx, expected.rcx, "{name}: RCX");
        assert_eq!(actual.rflags, expected.rflags, "{name}: RFLAGS");
    };

    // The helper commits RBP through GuestRegs, while the lowerer must also
    // synchronize the guest-RBP word saved under the trusted native frame.
    run_case("POP RBP", &[0x50, 0x5D, 0xF4], |regs| {
        regs.rax = 0x8877_6655_4433_2211;
        regs.rbp = 0x0123_4567_89AB_CDEF;
        regs.rsp = 0x11_0000;
        regs.rflags = 0x2 | 0x8D5;
    });

    // Intel specifies that POP RSP writes the loaded value after the otherwise
    // implicit increment, so the loaded value wins completely.
    run_case("POP RSP", &[0x50, 0x5C, 0xF4], |regs| {
        regs.rax = 0x12_3450;
        regs.rsp = 0x11_0000;
        regs.rflags = 0x2 | 0x8D5;
    });

    // Starting at 0x10_FFFE forces the 2-byte increment to carry into bit 16.
    // POP SP must retain that carry and replace only the final low 16 bits.
    run_case("POP SP carry", &[0x66, 0x50, 0x66, 0x5C, 0xF4], |regs| {
        regs.rax = 0x0123_4567_89AB_CDEF;
        regs.rsp = 0x11_0000;
        regs.rflags = 0x2 | 0x8D5;
    });

    for (name, code) in [
        ("faulting POP RSP", &[0x5C, 0xB9, 1, 0, 0, 0, 0xF4][..]),
        ("faulting POP SP", &[0x66, 0x5C, 0xB9, 1, 0, 0, 0, 0xF4][..]),
    ] {
        let mut fault = make_vcpu_code(code);
        let mut before = fault.get_regs().unwrap();
        before.rax = 0xA5A5_5A5A_1234_5678;
        before.rcx = 0xDEAD_BEEF;
        before.rsp = MEM_SIZE + 4;
        before.rflags = 0x2 | 0x8D5;
        fault.set_regs(&before).unwrap();
        fault.set_jit_mem(true);
        assert!(
            fault
                .jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error}")),
            "{name}: alias sequence must compile before the helper fault"
        );
        let after = fault.get_regs().unwrap();
        assert_eq!(after.rip, LOAD_ADDR, "{name}: restart PC");
        assert_eq!(after.rsp, before.rsp, "{name}: RSP must not commit");
        assert_eq!(after.rax, before.rax, "{name}: RAX");
        assert_eq!(after.rcx, before.rcx, "{name}: following MOV must not run");
        assert_eq!(after.rflags, before.rflags, "{name}: RFLAGS");
    }
}

/// A lift-through-call interpreter helper can semantically change guest RBP.
/// The native frame must retain hardware RBP as its trusted base while keeping
/// the prologue's saved guest value coherent for the final trampoline write-back.
#[test]
fn jit_callout_rbp_update_remains_coherent_with_state_backed_moves() {
    // call func; mov rax,rbp; hlt
    // func: mov rbp,0x123456789abcdef0; ret
    let code = [
        0xE8, 0x04, 0x00, 0x00, 0x00, 0x48, 0x89, 0xE8, 0xF4, 0x48, 0xBD, 0xF0, 0xDE, 0xBC, 0x9A,
        0x78, 0x56, 0x34, 0x12, 0xC3,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rbp = 0xDEAD_BEEF_CAFE_BABE;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interp = make_vcpu_code(&code);
    setup(&mut interp);
    run_interp(&mut interp);
    let expected = interp.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("callout plus state-backed RBP JIT"),
        "supported direct call and continuation must enter the native tier"
    );
    run_interp(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_eq!(actual.rax, expected.rax, "post-call MOV RAX,RBP");
    assert_eq!(actual.rbp, expected.rbp, "callee RBP result");
    assert_eq!(actual.rsp, expected.rsp, "CALL/RET stack balance");
    assert_eq!(actual.rflags, expected.rflags, "call continuation flags");
}

/// BSF/BSR define only ZF. The native tier must retain CF across each scan,
/// handle both zero and nonzero sources, preserve source/destination aliasing,
/// and produce the same defined results as the interpreter in a hot region.
#[test]
fn jit_bit_scans_preserve_undefined_flags_and_handle_zero_sources() {
    // loop:
    //   bsf r8,rax;  jnc fail; jz fail
    //   bsr r9,rbx;  jnc fail; jz fail
    //   bsf r10,rdx; jnc fail; jnz fail   (zero source)
    //   dec ecx; jnz loop
    //   hlt
    // fail: mov edi,1; hlt
    let code = [
        0x4C, 0x0F, 0xBC, 0xC0, 0x73, 0x17, 0x74, 0x15, 0x4C, 0x0F, 0xBD, 0xCB, 0x73, 0x0F, 0x74,
        0x0D, 0x4C, 0x0F, 0xBC, 0xD2, 0x73, 0x07, 0x75, 0x05, 0xFF, 0xC9, 0x75, 0xE4, 0xF4, 0xBF,
        0x01, 0x00, 0x00, 0x00, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x100;
        regs.rbx = 1u64 << 63;
        regs.rcx = 200;
        regs.rdx = 0;
        regs.rdi = 0;
        regs.r8 = u64::MAX;
        regs.r9 = u64::MAX;
        regs.r10 = 0xA5A5_A5A5_A5A5_A5A5;
        regs.rflags = 0x2 | 0x1 | 0x4 | 0x10 | 0x80 | 0x800;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interp = make_vcpu_code(&code);
    setup(&mut interp);
    run_interp(&mut interp);

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block().expect("JIT BSF/BSR hot region"),
        "well-formed register bit scans must enter the native tier"
    );
    run_interp(&mut jit);

    let expected = interp.get_regs().unwrap();
    let after = jit.get_regs().unwrap();
    assert_eq!(after.r8, expected.r8, "BSF nonzero result vs interpreter");
    assert_eq!(after.r9, expected.r9, "BSR nonzero result vs interpreter");
    assert_eq!(after.r8, 8, "lowest set-bit index");
    assert_eq!(after.r9, 63, "highest set-bit index");
    assert_eq!(after.rcx & 0xffff_ffff, 0, "loop count");
    assert_eq!(after.rdi, 0, "ZF/CF checks must avoid fail path");
    // R10 is architecturally undefined for a zero source and is intentionally
    // excluded from the cross-tier equality contract.
}

/// Register BT/BTS/BTR/BTC forms must enter the x86-64 native tier across
/// W16/W32/W64, preserve the emulator's deterministic values for undefined
/// status flags, and retain exact partial-register write semantics.
#[test]
fn jit_register_bit_tests_match_interpreter_across_widths_and_indices() {
    // bts ax,cx; btr r8d,31; btc r9,r10; bt ebx,edx
    // seto r11b; setz r12b; sets r13b; setp r14b; setc r15b
    // dec esi; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0xAB, 0xC8, 0x41, 0x0F, 0xBA, 0xF0, 0x1F, 0x4D, 0x0F, 0xBB, 0xD1, 0x0F, 0xA3,
        0xD3, 0x41, 0x0F, 0x90, 0xC3, 0x41, 0x0F, 0x94, 0xC4, 0x41, 0x0F, 0x98, 0xC5, 0x41, 0x0F,
        0x9A, 0xC6, 0x41, 0x0F, 0x92, 0xC7, 0xFF, 0xCE, 0x75, 0xD8, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xA5A5_A5A5_A5A5_0000;
        regs.rcx = 20; // W16 register index masks to bit 4.
        regs.r8 = u64::MAX;
        regs.r9 = 0;
        regs.r10 = 63;
        regs.rbx = 1 << 3;
        regs.rdx = 3;
        regs.rsi = 1;
        regs.r11 = u64::MAX;
        regs.r12 = u64::MAX;
        regs.r13 = u64::MAX;
        regs.r14 = u64::MAX;
        regs.r15 = u64::MAX;
        regs.rflags = 0x2 | 0x8D5; // all arithmetic status flags set.
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interp = make_vcpu_code(&code);
    setup(&mut interp);
    run_interp(&mut interp);

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block().expect("JIT register bit-test region"),
        "register bit-test loop must enter the native tier"
    );
    run_interp(&mut jit);

    let expected = interp.get_regs().unwrap();
    let after = jit.get_regs().unwrap();
    assert_eq!(after.rax, expected.rax, "BTS W16 partial write");
    assert_eq!(after.r8, expected.r8, "BTR W32 zero extension");
    assert_eq!(after.r9, expected.r9, "BTC W64 register index");
    assert_eq!(after.r11, expected.r11, "OF after final BT");
    assert_eq!(after.r12, expected.r12, "ZF after final BT");
    assert_eq!(after.r13, expected.r13, "SF after final BT");
    assert_eq!(after.r14, expected.r14, "PF after final BT");
    assert_eq!(after.r15, expected.r15, "CF after final BT");
    assert_eq!(after.rax, 0xA5A5_A5A5_A5A5_0010);
    assert_eq!(after.r8, 0x7FFF_FFFF);
    assert_eq!(after.r9, 1u64 << 63);
    assert_eq!(after.r11 & 0xFF, 1, "undefined OF preserved by policy");
    assert_eq!(after.r12 & 0xFF, 1, "undefined ZF preserved by policy");
    assert_eq!(after.r13 & 0xFF, 1, "undefined SF preserved by policy");
    assert_eq!(after.r14 & 0xFF, 1, "undefined PF preserved by policy");
    assert_eq!(after.r15 & 0xFF, 1, "BT extracts set bit into CF");
}

/// SSE4.2 CRC32 register accumulators with B1/B2/B4/B8 memory sources must be
/// fused through the MMU helper path without materializing the lifter's virtual
/// load value in a guest GPR. This exercises every architectural source width,
/// repeated helper calls, precise upper-bit clearing, and loop-carried CRCs.
#[test]
fn jit_memory_crc32c_all_widths_matches_interpreter() {
    if !std::is_x86_feature_detected!("sse4.2") {
        return;
    }

    // loop:
    //   crc32 r8d, byte  [rbx]
    //   crc32 r9d, word  [rbx+1]
    //   crc32 r10d,dword [rbx+3]
    //   crc32 r11, qword [rbx+7]
    //   dec ecx; jnz loop; hlt
    let code = [
        0xF2, 0x44, 0x0F, 0x38, 0xF0, 0x03, 0xF2, 0x66, 0x44, 0x0F, 0x38, 0xF1, 0x4B, 0x01, 0xF2,
        0x44, 0x0F, 0x38, 0xF1, 0x53, 0x03, 0xF2, 0x4C, 0x0F, 0x38, 0xF1, 0x5B, 0x07, 0xFF, 0xC9,
        0x75, 0xE0, 0xF4,
    ];
    const DATA_ADDR: u64 = 0x20_0000;
    const ITERATIONS: u64 = 7;
    let data = [
        0xA5, 0x34, 0x12, 0xEF, 0xBE, 0xAD, 0xDE, 0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01,
    ];
    let setup = |vcpu: &mut X86_64Vcpu, memory: &Arc<GuestMemoryMmap>| {
        memory.write_slice(&data, GuestAddress(DATA_ADDR)).unwrap();
        let mut regs = vcpu.get_regs().unwrap();
        regs.rbx = DATA_ADDR;
        regs.rcx = ITERATIONS;
        regs.r8 = 0xFFFF_FFFF_1020_3040;
        regs.r9 = 0xFFFF_FFFF_5060_7080;
        regs.r10 = 0xFFFF_FFFF_90A0_B0C0;
        regs.r11 = 0xFFFF_FFFF_D0E0_F001;
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
    };

    let (mut interp, interp_memory) = make_vcpu_mem(&code);
    setup(&mut interp, &interp_memory);
    run_interp(&mut interp);

    let (mut jit, jit_memory) = make_vcpu_mem(&code);
    setup(&mut jit, &jit_memory);
    assert!(
        jit.jit_try_block().expect("JIT memory CRC32 region"),
        "all memory-source CRC32 widths must enter the helper-backed native tier"
    );
    run_interp(&mut jit);

    let expected = interp.get_regs().unwrap();
    let actual = jit.get_regs().unwrap();
    assert_eq!(actual.r8, expected.r8, "CRC32 byte source");
    assert_eq!(actual.r9, expected.r9, "CRC32 word source");
    assert_eq!(actual.r10, expected.r10, "CRC32 dword source");
    assert_eq!(actual.r11, expected.r11, "CRC32 qword source");
    assert_eq!(actual.rcx, 0, "loop counter");
    assert_eq!(actual.r8 >> 32, 0, "byte-source destination zero extension");
    assert_eq!(actual.r9 >> 32, 0, "word-source destination zero extension");
    assert_eq!(
        actual.r10 >> 32,
        0,
        "dword-source destination zero extension"
    );
    assert_eq!(
        actual.r11 >> 32,
        0,
        "qword-source destination zero extension"
    );
}

/// A failed CRC32 memory read must unwind both fusion-owned stack slots and
/// return to the exact guest instruction without committing the accumulator.
#[test]
fn jit_memory_crc32c_fault_is_precise_and_noncommitting() {
    if !std::is_x86_feature_detected!("sse4.2") {
        return;
    }

    // loop: crc32 r8d, dword ptr [rbx]; dec ecx; jnz loop; hlt
    let mut vcpu = make_vcpu_code(&[
        0xF2, 0x44, 0x0F, 0x38, 0xF1, 0x03, 0xFF, 0xC9, 0x75, 0xF6, 0xF4,
    ]);
    let mut before = vcpu.get_regs().unwrap();
    before.rbx = MEM_SIZE + 0x1000;
    before.rcx = 1;
    before.r8 = 0xA5A5_A5A5_1234_5678;
    before.r9 = 0x1122_3344_5566_7788;
    before.rflags = 0x2 | 0x8D5;
    vcpu.set_regs(&before).unwrap();

    assert!(
        vcpu.jit_try_block().expect("JIT faulting memory CRC32"),
        "faulting memory CRC32 must compile before taking the guest-memory fault"
    );
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rip, LOAD_ADDR, "restart at faulting CRC32");
    assert_eq!(after.r8, before.r8, "CRC accumulator must not commit");
    assert_eq!(after.r9, before.r9, "unrelated GPR must survive unwind");
    assert_eq!(after.rcx, before.rcx, "post-load DEC must not execute");
    assert_eq!(after.rbx, before.rbx, "faulting address register");
    assert_eq!(after.rflags, before.rflags, "CRC32/fault path flags");
}

/// LFENCE/MFENCE/SFENCE are architectural side effects but have no GPR/RFLAGS
/// outputs. They must remain in optimized hot loops and execute natively.
#[test]
fn jit_x86_fence_family_matches_interpreter() {
    // loop: lfence; mfence; sfence; dec ecx; jnz loop; hlt
    let code = [
        0x0F, 0xAE, 0xE8, 0x0F, 0xAE, 0xF0, 0x0F, 0xAE, 0xF8, 0xFF, 0xC9, 0x75, 0xF3, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0123_4567_89AB_CDEF;
        regs.rcx = 100;
        regs.r8 = 0xFEDC_BA98_7654_3210;
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interp = make_vcpu_code(&code);
    setup(&mut interp);
    run_interp(&mut interp);

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block().expect("JIT x86 fence family"),
        "LFENCE/MFENCE/SFENCE loop must enter the native tier"
    );
    run_interp(&mut jit);

    let expected = interp.get_regs().unwrap();
    let actual = jit.get_regs().unwrap();
    assert_eq!(actual.rax, expected.rax);
    assert_eq!(actual.rcx, expected.rcx);
    assert_eq!(actual.r8, expected.r8);
    assert_eq!(actual.rflags, expected.rflags);
    assert_eq!(actual.rcx, 0);
}

/// CLDEMOTE is a non-faulting cache-placement hint and may be ignored. The JIT
/// therefore admits even an otherwise invalid guest address without exposing
/// it as a host memory access.
#[test]
fn jit_cldemote_ignored_hint_matches_interpreter_without_memory_jit() {
    // loop: cldemote byte ptr [rsp]; dec ecx; jnz loop; hlt
    let code = [0x0F, 0x1C, 0x04, 0x24, 0xFF, 0xC9, 0x75, 0xF8, 0xF4];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rsp = 0x0000_8000_0000_0000;
        regs.rcx = 100;
        regs.r8 = 0xA5A5_5A5A_C3C3_3C3C;
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
        vcpu.set_jit_mem(false);
    };

    let mut interp = make_vcpu_code(&code);
    setup(&mut interp);
    run_interp(&mut interp);

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block().expect("JIT CLDEMOTE loop"),
        "CLDEMOTE must not require memory-helper mode"
    );
    run_interp(&mut jit);

    let expected = interp.get_regs().unwrap();
    let actual = jit.get_regs().unwrap();
    assert_eq!(actual.rcx, 0);
    assert_eq!(actual.rsp, expected.rsp);
    assert_eq!(actual.r8, expected.r8);
    assert_eq!(actual.rflags, expected.rflags);
}

/// Native RDRAND/RDSEED retain architectural readiness retry behavior, exact
/// status flags, and 16/32/64-bit destination write semantics.
#[test]
fn jit_x86_random_all_widths_and_sources_reach_native_tier() {
    const INPUT: u64 = 0xA5A5_5A5A_C3C3_3C3C;
    const STATUS: u64 = (1 << 0) | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 11);

    for (name, seed, width, random) in [
        ("rdrand16", false, 16, &[0x66, 0x41, 0x0F, 0xC7, 0xF1][..]),
        ("rdrand32", false, 32, &[0x41, 0x0F, 0xC7, 0xF1][..]),
        ("rdrand64", false, 64, &[0x49, 0x0F, 0xC7, 0xF1][..]),
        ("rdseed16", true, 16, &[0x66, 0x41, 0x0F, 0xC7, 0xF9][..]),
        ("rdseed32", true, 32, &[0x41, 0x0F, 0xC7, 0xF9][..]),
        ("rdseed64", true, 64, &[0x49, 0x0F, 0xC7, 0xF9][..]),
    ] {
        if (seed && !std::is_x86_feature_detected!("rdseed"))
            || (!seed && !std::is_x86_feature_detected!("rdrand"))
        {
            continue;
        }

        // retry: rdrand/rdseed r9{w,d,}; jnc retry; hlt
        let mut code = random.to_vec();
        code.extend_from_slice(&[0x73, (-(random.len() as i8 + 2)) as u8, 0xF4]);
        let setup = |vcpu: &mut X86_64Vcpu| {
            let mut regs = vcpu.get_regs().unwrap();
            regs.r9 = INPUT;
            regs.rflags = 0x2 | STATUS;
            vcpu.set_regs(&regs).unwrap();
        };

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp);
        run_interp(&mut interp);

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit);
        assert!(
            jit.jit_try_block().expect("JIT hardware-random retry loop"),
            "{name}: host-supported X86Random must enter the native tier"
        );
        let actual = jit.get_regs().unwrap();
        let expected = interp.get_regs().unwrap();
        assert_eq!(actual.rip, LOAD_ADDR + random.len() as u64 + 2, "{name}");
        assert_eq!(
            actual.rflags & STATUS,
            1,
            "{name}: CF=1, other status clear"
        );
        assert_eq!(
            actual.rflags, expected.rflags,
            "{name}: architectural flags"
        );
        match width {
            16 => {
                assert_eq!(actual.r9 >> 16, INPUT >> 16);
                assert_eq!(expected.r9 >> 16, actual.r9 >> 16);
            }
            32 => {
                assert_eq!(actual.r9 >> 32, 0);
                assert_eq!(expected.r9 >> 32, 0);
            }
            64 => {}
            _ => unreachable!(),
        }
    }
}

/// XGETBV must consume the emulated XCR state, preserve RFLAGS/RCX, and enter
/// the native tier without exposing the host thread's XCR0.
#[test]
fn jit_xgetbv_reads_guest_xinuse_state_matches_interpreter() {
    const XCR0: u32 = 0xE7;
    const XINUSE: u64 = 0x25;
    const ITERATIONS: u32 = 100;

    // setup: mov ecx,0; mov eax,XCR0; xor edx,edx; xsetbv
    // loop setup: mov ecx,1; mov r8d,ITERATIONS
    // loop: xgetbv; dec r8d; jnz loop; hlt
    let mut code = vec![0xB9, 0, 0, 0, 0, 0xB8];
    code.extend_from_slice(&XCR0.to_le_bytes());
    code.extend_from_slice(&[
        0x31, 0xD2, // xor edx,edx
        0x0F, 0x01, 0xD1, // xsetbv
        0xB9, 0x01, 0, 0, 0, // mov ecx,1
        0x41, 0xB8,
    ]);
    code.extend_from_slice(&ITERATIONS.to_le_bytes());
    code.extend_from_slice(&[
        0x0F, 0x01, 0xD0, // xgetbv
        0x41, 0xFF, 0xC8, // dec r8d
        0x75, 0xF8, // jnz loop
        0xF4,
    ]);

    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut sregs = vcpu.get_sregs().unwrap();
        sregs.cr4 |= 1 << 18;
        vcpu.set_sregs(&sregs).unwrap();
        vcpu.set_xgetbv1_value(XINUSE);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = u64::MAX;
        regs.rdx = u64::MAX;
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interp = make_vcpu_code(&code);
    setup(&mut interp);
    run_interp(&mut interp);

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    // Execute the straight-line XSETBV setup in the interpreter so the hot
    // region begins at the selector/counter setup and loops over XGETBV.
    for _ in 0..4 {
        assert!(jit.step().expect("XGETBV setup instruction").is_none());
    }
    assert!(
        jit.jit_try_block().expect("JIT XGETBV loop"),
        "state-backed XGETBV loop must enter the native tier"
    );
    run_interp(&mut jit);

    let expected = interp.get_regs().unwrap();
    let actual = jit.get_regs().unwrap();
    assert_eq!(actual.rax, XINUSE);
    assert_eq!(actual.rdx, 0);
    assert_eq!(actual.rcx, 1, "XGETBV preserves the selector");
    assert_eq!(actual.r8 & 0xFFFF_FFFF, 0);
    assert_eq!(actual.rax, expected.rax);
    assert_eq!(actual.rdx, expected.rdx);
    assert_eq!(actual.rcx, expected.rcx);
    assert_eq!(actual.rflags, expected.rflags);
}

/// XSETBV validates and commits guest XCR0 in native code, then returns at the
/// following instruction so the remainder is decoded/compiled under the new
/// extended-state policy. EDX:EAX, ECX, and RFLAGS remain unchanged.
#[test]
fn jit_xsetbv_commits_guest_state_and_forces_next_instruction_handoff() {
    // xsetbv; loop: xgetbv; dec r8d; jnz loop; hlt
    //
    // The conditional edge makes the entry a compilable region rather than a
    // straight-line trap frontier. XSETBV must nevertheless terminate native
    // execution before XGETBV because changing XCR0 changes decode policy.
    let code = [
        0x0F, 0x01, 0xD1, // xsetbv
        0x0F, 0x01, 0xD0, // xgetbv
        0x41, 0xFF, 0xC8, // dec r8d
        0x75, 0xF8, // jnz xgetbv
        0xF4,
    ];

    for (name, value, apx_enabled) in [
        ("x87", 1u64, false),
        ("avx", 7, false),
        ("avx512", 0xE7, false),
        ("apx", 0x0008_00E7, true),
    ] {
        let setup = |vcpu: &mut X86_64Vcpu| {
            vcpu.set_apx_enabled(apx_enabled);
            let mut sregs = vcpu.get_sregs().unwrap();
            sregs.cr4 |= 1 << 18;
            vcpu.set_sregs(&sregs).unwrap();
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = 0xA5A5_A5A5_0000_0000 | value as u32 as u64;
            regs.rcx = 0x5A5A_5A5A_0000_0000;
            regs.rdx = 0xC3C3_C3C3_0000_0000 | (value >> 32) as u32 as u64;
            regs.r8 = 1;
            regs.rflags = 0x2 | 0x8D5;
            vcpu.set_regs(&regs).unwrap();
            regs
        };

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp);
        run_interp(&mut interp);

        let mut jit = make_vcpu_code(&code);
        let inputs = setup(&mut jit);
        assert!(
            jit.jit_try_block().expect("JIT XSETBV region"),
            "{name}: state-backed XSETBV must enter the native tier"
        );
        let handoff = jit.get_regs().unwrap();
        assert_eq!(handoff.rip, LOAD_ADDR + 3, "{name}: next-instruction PC");
        assert_eq!(handoff.rax, inputs.rax, "{name}: RAX preserved");
        assert_eq!(handoff.rcx, inputs.rcx, "{name}: RCX preserved");
        assert_eq!(handoff.rdx, inputs.rdx, "{name}: RDX preserved");
        assert_eq!(handoff.rflags, inputs.rflags, "{name}: RFLAGS preserved");

        assert!(jit.step().expect("post-XSETBV XGETBV").is_none());
        let actual = jit.get_regs().unwrap();
        let expected = interp.get_regs().unwrap();
        assert_eq!(actual.rax, value as u32 as u64, "{name}: XCR0 low");
        assert_eq!(actual.rdx, (value >> 32) as u32 as u64, "{name}: XCR0 high");
        assert_eq!(actual.rcx, inputs.rcx, "{name}: selector preserved");
        assert_eq!(actual.rflags, inputs.rflags, "{name}: flags preserved");
        assert_eq!(actual.rax, expected.rax, "{name}: interpreter low");
        assert_eq!(actual.rdx, expected.rdx, "{name}: interpreter high");
    }
}

/// APX NF count instructions have no architectural flag side effects. The JIT
/// re-encodes them as legacy host count instructions wrapped by PUSHFQ/POPFQ,
/// so each instruction must retain incoming CF while producing the same count
/// result as the interpreter.
#[test]
fn jit_apx_nf_counts_preserve_flags_and_enter_native_tier() {
    if !(std::is_x86_feature_detected!("popcnt")
        && std::is_x86_feature_detected!("bmi1")
        && std::is_x86_feature_detected!("lzcnt"))
    {
        return;
    }

    // loop:
    //   {nf} popcnt r8,rax; jnc fail
    //   {nf} lzcnt  r8,rax; jnc fail
    //   {nf} tzcnt  r8,rax; jnc fail
    //   dec ecx; jnz loop
    //   hlt
    // fail: mov edi,1; hlt
    let code = [
        0x62, 0x74, 0xFC, 0x0C, 0x88, 0xC0, 0x73, 0x15, 0x62, 0x74, 0xFC, 0x0C, 0xF5, 0xC0, 0x73,
        0x0D, 0x62, 0x74, 0xFC, 0x0C, 0xF4, 0xC0, 0x73, 0x05, 0xFF, 0xC9, 0x75, 0xE4, 0xF4, 0xBF,
        0x01, 0x00, 0x00, 0x00, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        vcpu.set_apx_enabled(true);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x100;
        regs.rcx = 200;
        regs.rdi = 0;
        regs.r8 = u64::MAX;
        regs.rflags = 0x2 | 0x1 | 0x4 | 0x10 | 0x40 | 0x80 | 0x800;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interp = make_vcpu_code(&code);
    setup(&mut interp);
    run_interp(&mut interp);

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block().expect("JIT APX NF count hot region"),
        "well-formed APX NF register counts must enter the native tier"
    );
    run_interp(&mut jit);

    let expected = interp.get_regs().unwrap();
    let after = jit.get_regs().unwrap();
    assert_eq!(after.r8, expected.r8, "count result vs interpreter");
    assert_eq!(after.r8, 8, "final TZCNT result");
    assert_eq!(after.rcx & 0xffff_ffff, 0, "loop count");
    assert_eq!(after.rdi, 0, "each NF count must preserve incoming CF");
    assert_ne!(after.rflags & 1, 0, "DEC and every NF count preserve CF");
}

/// Legacy POPCNT/TZCNT/LZCNT must enter the native tier with their distinct
/// flag contracts. A dependency-free XOR after the loop establishes known
/// incoming status flags; the final count operation then either replaces all
/// status flags (POPCNT) or merges only CF/ZF while retaining the undefined
/// flags (TZCNT/LZCNT).
#[test]
fn jit_legacy_counts_match_interpreter_results_and_exact_status_flags() {
    const STATUS_MASK: u64 = 0x08D5;
    for (name, opcode, input, expected_result, expected_status, supported) in [
        (
            "popcnt",
            0xB8,
            0,
            0,
            0x40,
            std::is_x86_feature_detected!("popcnt"),
        ),
        (
            "tzcnt",
            0xBC,
            0,
            64,
            0x05,
            std::is_x86_feature_detected!("bmi1"),
        ),
        (
            "lzcnt",
            0xBD,
            1u64 << 63,
            0,
            0x44,
            std::is_x86_feature_detected!("lzcnt"),
        ),
    ] {
        if !supported {
            continue;
        }

        // loop: dec ecx; jnz loop
        //       xor r9d,r9d
        //       <count> r8,rax
        //       hlt
        let code = [
            0xFF, 0xC9, 0x75, 0xFC, 0x45, 0x31, 0xC9, 0xF3, 0x4C, 0x0F, opcode, 0xC0, 0xF4,
        ];
        let setup = |vcpu: &mut X86_64Vcpu| {
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = input;
            regs.rcx = 200;
            regs.r8 = u64::MAX;
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp);
        run_interp(&mut interp);

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("JIT legacy {name}: {error:?}")),
            "legacy {name} loop must enter the native tier"
        );
        run_interp(&mut jit);

        let expected = interp.get_regs().unwrap();
        let after = jit.get_regs().unwrap();
        assert_eq!(after.r8, expected.r8, "{name}: result vs interpreter");
        assert_eq!(after.r8, expected_result, "{name}: architectural result");
        assert_eq!(after.rcx & 0xFFFF_FFFF, 0, "{name}: loop count");
        assert_eq!(
            after.rflags & STATUS_MASK,
            expected.rflags & STATUS_MASK,
            "{name}: native status flags vs interpreter"
        );
        assert_eq!(
            after.rflags & STATUS_MASK,
            expected_status,
            "{name}: exact architectural status flags"
        );
    }
}

/// Memory-source count instructions use the MMU helper for the load and then
/// consume a caller-owned stack word. This must preserve fault restart state,
/// legacy 16-bit destination upper bits, and each instruction's distinct flag
/// contract, including APX NF's complete flag preservation.
#[test]
fn jit_memory_source_counts_match_interpreter_partial_writes_flags_and_faults() {
    const DATA: u64 = 0x20_0000;

    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        data: u64,
        dst: u8,
        initial_dst: u64,
        expected_dst: u64,
        supported: bool,
    }

    let cases = [
        Case {
            name: "POPCNT r8w,[rbx]",
            instruction: &[0xF3, 0x66, 0x44, 0x0F, 0xB8, 0x03],
            apx: false,
            data: 0xF0F0,
            dst: 8,
            initial_dst: 0xAABB_CCDD_EEFF_7788,
            expected_dst: 0xAABB_CCDD_EEFF_0008,
            supported: std::is_x86_feature_detected!("popcnt"),
        },
        Case {
            name: "TZCNT r9d,[rbx]",
            instruction: &[0xF3, 0x44, 0x0F, 0xBC, 0x0B],
            apx: false,
            data: 0,
            dst: 9,
            initial_dst: u64::MAX,
            expected_dst: 32,
            supported: std::is_x86_feature_detected!("bmi1"),
        },
        Case {
            name: "LZCNT r15,[rbx]",
            instruction: &[0xF3, 0x4C, 0x0F, 0xBD, 0x3B],
            apx: false,
            data: 1,
            dst: 15,
            initial_dst: u64::MAX,
            expected_dst: 63,
            supported: std::is_x86_feature_detected!("lzcnt"),
        },
        Case {
            name: "APX NF LZCNT r8,[rbx]",
            instruction: &[0x62, 0x74, 0xFC, 0x0C, 0xF5, 0x03],
            apx: true,
            data: 1 << 63,
            dst: 8,
            initial_dst: u64::MAX,
            expected_dst: 0,
            supported: std::is_x86_feature_detected!("lzcnt"),
        },
    ];

    let read_dst = |regs: &Registers, index: u8| match index {
        8 => regs.r8,
        9 => regs.r9,
        15 => regs.r15,
        _ => unreachable!(),
    };
    for case in cases {
        if !case.supported {
            continue;
        }
        let mut code = case.instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, memory: &Arc<GuestMemoryMmap>| {
            memory.write_obj(case.data, GuestAddress(DATA)).unwrap();
            vcpu.set_apx_enabled(case.apx);
            let mut regs = vcpu.get_regs().unwrap();
            regs.rbx = DATA;
            match case.dst {
                8 => regs.r8 = case.initial_dst,
                9 => regs.r9 = case.initial_dst,
                15 => regs.r15 = case.initial_dst,
                _ => unreachable!(),
            }
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        let (mut interp, interp_mem) = make_vcpu_mem(&code);
        setup(&mut interp, &interp_mem);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the helper-backed native tier",
            case.name
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(
            read_dst(&actual, case.dst),
            read_dst(&expected, case.dst),
            "{} result vs interpreter",
            case.name
        );
        assert_eq!(
            read_dst(&actual, case.dst),
            case.expected_dst,
            "{} architectural result",
            case.name
        );
        assert_eq!(actual.rbx, DATA, "{} address base", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }

    if std::is_x86_feature_detected!("popcnt") {
        let code = [0xF3, 0x4C, 0x0F, 0xB8, 0x03, 0xF4]; // popcnt r8,[rbx]
        let (mut fault, _) = make_vcpu_mem(&code);
        let mut before = fault.get_regs().unwrap();
        before.rbx = MEM_SIZE + 0x1000;
        before.r8 = 0xA5A5_5A5A_A5A5_5A5A;
        before.rflags = 0xCD7;
        fault.set_regs(&before).unwrap();
        assert!(
            fault
                .jit_try_block()
                .expect("faulting scalar count memory-source JIT"),
            "a count load must compile before precise deoptimization"
        );
        let after = fault.get_regs().unwrap();
        assert_eq!(after.rbx, before.rbx, "fault must preserve address base");
        assert_eq!(after.r8, before.r8, "fault must preserve destination");
        assert_eq!(after.rflags, before.rflags, "fault must preserve RFLAGS");
        assert_eq!(after.rip, LOAD_ADDR, "fault must restart current PC");
    }
}

/// Memory-source BSF/BSR pairs use one precise MMU-helper load followed by a
/// native scan of caller-owned stack storage. Defined ZF behavior, the
/// emulator's retained undefined status flags, partial destination writes, and
/// fault restart state must match the interpreter.
#[test]
fn jit_memory_source_bit_scans_match_interpreter_partial_writes_flags_and_faults() {
    const DATA: u64 = 0x20_0000;

    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        data: u64,
        dst: u8,
        initial_dst: u64,
        expected_dst: Option<u64>,
    }

    let cases = [
        Case {
            name: "BSF r8w,[rbx]",
            instruction: &[0x66, 0x44, 0x0F, 0xBC, 0x03],
            data: 0x0100,
            dst: 8,
            initial_dst: 0xAABB_CCDD_EEFF_7788,
            expected_dst: Some(0xAABB_CCDD_EEFF_0008),
        },
        Case {
            name: "BSR r9d,[rbx]",
            instruction: &[0x44, 0x0F, 0xBD, 0x0B],
            data: 0x8000_0000,
            dst: 9,
            initial_dst: u64::MAX,
            expected_dst: Some(31),
        },
        Case {
            name: "BSF r15,[rbx]",
            instruction: &[0x4C, 0x0F, 0xBC, 0x3B],
            data: 1 << 63,
            dst: 15,
            initial_dst: u64::MAX,
            expected_dst: Some(63),
        },
        Case {
            name: "BSR r8,[rbx] zero source",
            instruction: &[0x4C, 0x0F, 0xBD, 0x03],
            data: 0,
            dst: 8,
            initial_dst: 0xA5A5_5A5A_1357_2468,
            // The ISA leaves this result undefined; compare only with Rax's
            // interpreter policy rather than assigning an architectural value.
            expected_dst: None,
        },
    ];

    let read_dst = |regs: &Registers, index: u8| match index {
        8 => regs.r8,
        9 => regs.r9,
        15 => regs.r15,
        _ => unreachable!(),
    };
    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, memory: &Arc<GuestMemoryMmap>| {
            memory.write_obj(case.data, GuestAddress(DATA)).unwrap();
            let mut regs = vcpu.get_regs().unwrap();
            regs.rbx = DATA;
            match case.dst {
                8 => regs.r8 = case.initial_dst,
                9 => regs.r9 = case.initial_dst,
                15 => regs.r15 = case.initial_dst,
                _ => unreachable!(),
            }
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        let (mut interp, interp_mem) = make_vcpu_mem(&code);
        setup(&mut interp, &interp_mem);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the helper-backed native tier",
            case.name
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(
            read_dst(&actual, case.dst),
            read_dst(&expected, case.dst),
            "{} result vs interpreter",
            case.name
        );
        if let Some(architectural_result) = case.expected_dst {
            assert_eq!(
                read_dst(&actual, case.dst),
                architectural_result,
                "{} architectural result",
                case.name
            );
        }
        assert_eq!(actual.rbx, DATA, "{} address base", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }

    let code = [0x4C, 0x0F, 0xBC, 0x03, 0xF4]; // bsf r8,[rbx]
    let (mut fault, _) = make_vcpu_mem(&code);
    let mut before = fault.get_regs().unwrap();
    before.rbx = MEM_SIZE + 0x1000;
    before.r8 = 0xA5A5_5A5A_A5A5_5A5A;
    before.rflags = 0xCD7;
    fault.set_regs(&before).unwrap();
    assert!(
        fault
            .jit_try_block()
            .expect("faulting bit-scan memory-source JIT"),
        "a bit-scan load must compile before precise deoptimization"
    );
    let after = fault.get_regs().unwrap();
    assert_eq!(after.rbx, before.rbx, "fault must preserve address base");
    assert_eq!(after.r8, before.r8, "fault must preserve destination");
    assert_eq!(after.rflags, before.rflags, "fault must preserve RFLAGS");
    assert_eq!(after.rip, LOAD_ADDR, "fault must restart current PC");
}

/// Immediate memory BT is non-modifying: the helper-backed native path must
/// merge only CF, retain every other status flag and GPR, leave memory intact,
/// and restart precisely if the operand load faults.
#[test]
fn jit_memory_source_immediate_bit_tests_match_interpreter_flags_and_faults() {
    const DATA: u64 = 0x20_0000;
    for (name, instruction, data, expected_cf) in [
        (
            "BT word [rbx],15",
            &[0x66, 0x0F, 0xBA, 0x23, 0x0F][..],
            0x8000u64,
            true,
        ),
        ("BT dword [rbx],7", &[0x0F, 0xBA, 0x23, 0x07][..], 0, false),
        (
            "BT qword [rbx],63",
            &[0x48, 0x0F, 0xBA, 0x23, 0x3F][..],
            1u64 << 63,
            true,
        ),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, memory: &Arc<GuestMemoryMmap>| {
            memory.write_obj(data, GuestAddress(DATA)).unwrap();
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = 0xA5A5_5A5A_1357_2468;
            regs.rbx = DATA;
            regs.r8 = 0x0123_4567_89AB_CDEF;
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        let (mut interp, interp_mem) = make_vcpu_mem(&code);
        setup(&mut interp, &interp_mem);
        assert!(interp.step().unwrap().is_none(), "{name} interpreter");
        let expected = interp.get_regs().unwrap();

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error:?}")),
            "{name} must enter the helper-backed native tier"
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(actual.rax, expected.rax, "{name}: RAX scratch preservation");
        assert_eq!(actual.rbx, expected.rbx, "{name}: address base");
        assert_eq!(actual.r8, expected.r8, "{name}: unrelated GPR");
        assert_eq!(actual.rflags, expected.rflags, "{name}: RFLAGS");
        assert_eq!(actual.rflags & 1 != 0, expected_cf, "{name}: CF");
        assert_eq!(actual.rip, expected.rip, "{name}: RIP");
        assert_eq!(
            jit_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
            data,
            "{name}: memory must remain unchanged"
        );
    }

    let code = [0x48, 0x0F, 0xBA, 0x23, 0x05, 0xF4]; // bt qword [rbx],5
    let (mut fault, _) = make_vcpu_mem(&code);
    let mut before = fault.get_regs().unwrap();
    before.rax = 0xA5A5_5A5A_1357_2468;
    before.rbx = MEM_SIZE + 0x1000;
    before.r8 = 0x0123_4567_89AB_CDEF;
    before.rflags = 0xCD7;
    fault.set_regs(&before).unwrap();
    assert!(
        fault
            .jit_try_block()
            .expect("faulting immediate memory BT JIT"),
        "BT must compile before its precise helper fault"
    );
    let after = fault.get_regs().unwrap();
    assert_eq!(after.rax, before.rax, "fault must preserve RAX");
    assert_eq!(after.rbx, before.rbx, "fault must preserve address base");
    assert_eq!(after.r8, before.r8, "fault must preserve unrelated GPRs");
    assert_eq!(after.rflags, before.rflags, "fault must preserve RFLAGS");
    assert_eq!(after.rip, LOAD_ADDR, "fault must restart current PC");
}

/// Non-locked immediate memory BTS/BTR/BTC must publish the updated operand
/// before committing CF. The helper-backed native path is differential-tested
/// across all operand widths/actions, while an inaccessible operand verifies
/// precise restart with no register or flag commit.
#[test]
fn jit_memory_destination_immediate_bit_updates_match_interpreter_and_faults() {
    const DATA: u64 = 0x20_0000;
    for (name, instruction, initial, expected_value, expected_cf) in [
        (
            "BTS word [rbx],15",
            &[0x66, 0x0F, 0xBA, 0x2B, 0x0F][..],
            0x1122_3344_5566_0000u64,
            0x1122_3344_5566_8000u64,
            false,
        ),
        (
            "BTR dword [rbx],7",
            &[0x0F, 0xBA, 0x33, 0x07][..],
            0x1122_3344_FFFF_FFFFu64,
            0x1122_3344_FFFF_FF7Fu64,
            true,
        ),
        (
            "BTC qword [rbx],63",
            &[0x48, 0x0F, 0xBA, 0x3B, 0x3F][..],
            0x8000_0000_0000_0000u64,
            0,
            true,
        ),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, memory: &Arc<GuestMemoryMmap>| {
            memory.write_obj(initial, GuestAddress(DATA)).unwrap();
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = 0xA5A5_5A5A_1357_2468;
            regs.rbx = DATA;
            regs.r8 = 0x0123_4567_89AB_CDEF;
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        let (mut interp, interp_mem) = make_vcpu_mem(&code);
        setup(&mut interp, &interp_mem);
        assert!(interp.step().unwrap().is_none(), "{name} interpreter");
        let expected = interp.get_regs().unwrap();
        let expected_memory = interp_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap();
        assert_eq!(
            expected_memory, expected_value,
            "{name}: architectural memory"
        );

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        let ran_native = jit
            .jit_try_block()
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(
            ran_native,
            "{name} must enter the helper-backed native tier:\n{}",
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(actual.rax, expected.rax, "{name}: RAX scratch preservation");
        assert_eq!(actual.rbx, expected.rbx, "{name}: address base");
        assert_eq!(actual.r8, expected.r8, "{name}: unrelated GPR");
        assert_eq!(actual.rflags, expected.rflags, "{name}: RFLAGS");
        assert_eq!(actual.rflags & 1 != 0, expected_cf, "{name}: CF");
        assert_eq!(actual.rip, expected.rip, "{name}: RIP");
        assert_eq!(
            jit_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
            expected_memory,
            "{name}: memory vs interpreter"
        );
    }

    let code = [0x48, 0x0F, 0xBA, 0x2B, 0x05, 0xF4]; // bts qword [rbx],5
    let (mut fault, _) = make_vcpu_mem(&code);
    let mut before = fault.get_regs().unwrap();
    before.rax = 0xA5A5_5A5A_1357_2468;
    before.rbx = MEM_SIZE + 0x1000;
    before.r8 = 0x0123_4567_89AB_CDEF;
    before.rflags = 0xCD7;
    fault.set_regs(&before).unwrap();
    assert!(
        fault
            .jit_try_block()
            .expect("faulting immediate memory BTS JIT"),
        "BTS must compile before its precise helper fault"
    );
    let after = fault.get_regs().unwrap();
    assert_eq!(after.rax, before.rax, "fault must preserve RAX");
    assert_eq!(after.rbx, before.rbx, "fault must preserve address base");
    assert_eq!(after.r8, before.r8, "fault must preserve unrelated GPRs");
    assert_eq!(after.rflags, before.rflags, "fault must preserve RFLAGS");
    assert_eq!(after.rip, LOAD_ADDR, "fault must restart current PC");
}

/// Two-operand IMUL with a memory source must consume the helper-staged value,
/// preserve partial-register semantics, and leave all architectural state
/// uncommitted when the source load faults. Only CF and OF are architecturally
/// defined after a successful IMUL and therefore participate in flag equality.
#[test]
fn jit_two_operand_memory_imul_matches_interpreter_and_faults_precisely() {
    const DATA: u64 = 0x20_0000;
    const DEFINED_FLAGS: u64 = 1 | (1 << 11);
    for (name, instruction, destination, initial, memory, expected_value) in [
        (
            "IMUL AX,word [RBX]",
            &[0x66, 0x0F, 0xAF, 0x03][..],
            0u8,
            0xA5A5_5A5A_1357_0007u64,
            0x0000_0000_0000_FFFDu64,
            0xA5A5_5A5A_1357_FFEBu64,
        ),
        (
            "IMUL R8D,dword [RBX]",
            &[0x44, 0x0F, 0xAF, 0x03][..],
            1,
            2,
            0x0000_0000_8000_0000,
            0,
        ),
        (
            "IMUL R9,qword [RBX]",
            &[0x4C, 0x0F, 0xAF, 0x0B][..],
            2,
            3,
            7,
            21,
        ),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, guest_mem: &Arc<GuestMemoryMmap>| {
            guest_mem.write_obj(memory, GuestAddress(DATA)).unwrap();
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = 0x0123_4567_89AB_CDEF;
            regs.r8 = 0x1122_3344_5566_7788;
            regs.r9 = 0x8877_6655_4433_2211;
            match destination {
                0 => regs.rax = initial,
                1 => regs.r8 = initial,
                2 => regs.r9 = initial,
                _ => unreachable!(),
            }
            regs.rbx = DATA;
            regs.r10 = 0x0F0E_0D0C_0B0A_0908;
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        let (mut interp, interp_mem) = make_vcpu_mem(&code);
        setup(&mut interp, &interp_mem);
        assert!(interp.step().unwrap().is_none(), "{name} interpreter");
        let expected = interp.get_regs().unwrap();
        let expected_destination = match destination {
            0 => expected.rax,
            1 => expected.r8,
            2 => expected.r9,
            _ => unreachable!(),
        };
        assert_eq!(
            expected_destination, expected_value,
            "{name}: reference value"
        );

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        let ran_native = jit
            .jit_try_block()
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(
            ran_native,
            "{name} must enter the helper-backed native tier:\n{}",
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(actual.rax, expected.rax, "{name}: RAX");
        assert_eq!(actual.r8, expected.r8, "{name}: R8");
        assert_eq!(actual.r9, expected.r9, "{name}: R9");
        assert_eq!(actual.rbx, expected.rbx, "{name}: address base");
        assert_eq!(actual.r10, expected.r10, "{name}: unrelated GPR");
        assert_eq!(
            actual.rflags & DEFINED_FLAGS,
            expected.rflags & DEFINED_FLAGS,
            "{name}: defined CF/OF"
        );
        assert_eq!(actual.rip, expected.rip, "{name}: RIP");
        assert_eq!(
            jit_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
            memory,
            "{name}: source memory is unchanged"
        );
    }

    let code = [0x4C, 0x0F, 0xAF, 0x03, 0xF4]; // imul r8,qword [rbx]
    let (mut fault, _) = make_vcpu_mem(&code);
    let mut before = fault.get_regs().unwrap();
    before.rax = 0x0123_4567_89AB_CDEF;
    before.rbx = MEM_SIZE + 0x1000;
    before.r8 = 0x1122_3344_5566_7788;
    before.r9 = 0x8877_6655_4433_2211;
    before.rflags = 0xCD7;
    fault.set_regs(&before).unwrap();
    assert!(
        fault
            .jit_try_block()
            .expect("faulting two-operand memory IMUL JIT"),
        "IMUL must compile before its precise helper fault"
    );
    let after = fault.get_regs().unwrap();
    assert_eq!(after.rax, before.rax, "fault must preserve RAX");
    assert_eq!(after.rbx, before.rbx, "fault must preserve address base");
    assert_eq!(after.r8, before.r8, "fault must preserve destination");
    assert_eq!(after.r9, before.r9, "fault must preserve unrelated GPRs");
    assert_eq!(after.rflags, before.rflags, "fault must preserve RFLAGS");
    assert_eq!(after.rip, LOAD_ADDR, "fault must restart current PC");
}

/// Immediate IMUL with a memory source must retain its 69/6B encoding class,
/// consume the helper-staged value, and commit no architectural state when the
/// source load faults. Only CF and OF are defined after successful IMUL.
#[test]
fn jit_memory_immediate_imul_matches_interpreter_and_faults_precisely() {
    const DATA: u64 = 0x20_0000;
    const DEFINED_FLAGS: u64 = 1 | (1 << 11);
    for (name, instruction, destination, initial, memory, expected_value) in [
        (
            "IMUL AX,word [RBX],imm16",
            &[0x66, 0x69, 0x03, 0x34, 0x12][..],
            0u8,
            0xA5A5_5A5A_1357_2468u64,
            3u64,
            0xA5A5_5A5A_1357_369Cu64,
        ),
        (
            "IMUL R8D,dword [RBX],imm8",
            &[0x44, 0x6B, 0x03, 0xFD][..],
            1,
            0x1122_3344_5566_7788,
            0x0000_0000_8000_0000,
            0x0000_0000_8000_0000,
        ),
        (
            "IMUL R9,qword [RBX],imm32",
            &[0x4C, 0x69, 0x0B, 0x78, 0x56, 0x34, 0x12][..],
            2,
            0x8877_6655_4433_2211,
            7,
            0x0000_0000_7F6E_5D48,
        ),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, guest_mem: &Arc<GuestMemoryMmap>| {
            guest_mem.write_obj(memory, GuestAddress(DATA)).unwrap();
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = 0x0123_4567_89AB_CDEF;
            regs.r8 = 0x1122_3344_5566_7788;
            regs.r9 = 0x8877_6655_4433_2211;
            match destination {
                0 => regs.rax = initial,
                1 => regs.r8 = initial,
                2 => regs.r9 = initial,
                _ => unreachable!(),
            }
            regs.rbx = DATA;
            regs.r10 = 0x0F0E_0D0C_0B0A_0908;
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        let (mut interp, interp_mem) = make_vcpu_mem(&code);
        setup(&mut interp, &interp_mem);
        assert!(interp.step().unwrap().is_none(), "{name} interpreter");
        let expected = interp.get_regs().unwrap();
        let expected_destination = match destination {
            0 => expected.rax,
            1 => expected.r8,
            2 => expected.r9,
            _ => unreachable!(),
        };
        assert_eq!(
            expected_destination, expected_value,
            "{name}: reference value"
        );

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        let ran_native = jit
            .jit_try_block()
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(
            ran_native,
            "{name} must enter the helper-backed native tier:\n{}",
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(actual.rax, expected.rax, "{name}: RAX");
        assert_eq!(actual.r8, expected.r8, "{name}: R8");
        assert_eq!(actual.r9, expected.r9, "{name}: R9");
        assert_eq!(actual.rbx, expected.rbx, "{name}: address base");
        assert_eq!(actual.r10, expected.r10, "{name}: unrelated GPR");
        assert_eq!(
            actual.rflags & DEFINED_FLAGS,
            expected.rflags & DEFINED_FLAGS,
            "{name}: defined CF/OF"
        );
        assert_eq!(actual.rip, expected.rip, "{name}: RIP");
        assert_eq!(
            jit_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
            memory,
            "{name}: source memory is unchanged"
        );
    }

    let code = [0x4C, 0x69, 0x03, 0x78, 0x56, 0x34, 0x12, 0xF4];
    let (mut fault, _) = make_vcpu_mem(&code);
    let mut before = fault.get_regs().unwrap();
    before.rax = 0x0123_4567_89AB_CDEF;
    before.rbx = MEM_SIZE + 0x1000;
    before.r8 = 0x1122_3344_5566_7788;
    before.r9 = 0x8877_6655_4433_2211;
    before.rflags = 0xCD7;
    fault.set_regs(&before).unwrap();
    assert!(
        fault
            .jit_try_block()
            .expect("faulting immediate memory IMUL JIT"),
        "immediate IMUL must compile before its precise helper fault"
    );
    let after = fault.get_regs().unwrap();
    assert_eq!(after.rax, before.rax, "fault must preserve RAX");
    assert_eq!(after.rbx, before.rbx, "fault must preserve address base");
    assert_eq!(after.r8, before.r8, "fault must preserve destination");
    assert_eq!(after.r9, before.r9, "fault must preserve unrelated GPRs");
    assert_eq!(after.rflags, before.rflags, "fault must preserve RFLAGS");
    assert_eq!(after.rip, LOAD_ADDR, "fault must restart current PC");
}

/// Implicit widening MUL/IMUL with a memory source must consume the
/// helper-staged operand, retain partial-register semantics, and leave both
/// RAX and RDX uncommitted if the source load faults. Only CF and OF are
/// architecturally defined after a successful multiply.
#[test]
fn jit_widening_memory_multiply_matches_interpreter_and_faults_precisely() {
    const DATA: u64 = 0x20_0000;
    const DEFINED_FLAGS: u64 = 1 | (1 << 11);
    for (name, instruction, initial_rax, initial_rdx, memory, expected_rax, expected_rdx) in [
        (
            "MUL byte [RBX]",
            &[0xF6, 0x23][..],
            0xA5A5_5A5A_1357_0012u64,
            0x1122_3344_5566_7788u64,
            0x10u64,
            0xA5A5_5A5A_1357_0120u64,
            0x1122_3344_5566_7788u64,
        ),
        (
            "IMUL word [RBX]",
            &[0x66, 0xF7, 0x2B][..],
            0xA5A5_5A5A_1357_0007,
            0x1122_3344_5566_7788,
            0x0000_0000_0000_FFFD,
            0xA5A5_5A5A_1357_FFEB,
            0x1122_3344_5566_FFFF,
        ),
        (
            "MUL dword [RBX]",
            &[0xF7, 0x23][..],
            0xA5A5_5A5A_FFFF_FFFF,
            0x1122_3344_5566_7788,
            2,
            0x0000_0000_FFFF_FFFE,
            1,
        ),
        (
            "IMUL qword [RBX]",
            &[0x48, 0xF7, 0x2B][..],
            3,
            0x1122_3344_5566_7788,
            0xFFFF_FFFF_FFFF_FFF9,
            0xFFFF_FFFF_FFFF_FFEB,
            0xFFFF_FFFF_FFFF_FFFF,
        ),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, guest_mem: &Arc<GuestMemoryMmap>| {
            guest_mem.write_obj(memory, GuestAddress(DATA)).unwrap();
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = initial_rax;
            regs.rdx = initial_rdx;
            regs.rbx = DATA;
            regs.r8 = 0x0123_4567_89AB_CDEF;
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        let (mut interp, interp_mem) = make_vcpu_mem(&code);
        setup(&mut interp, &interp_mem);
        assert!(interp.step().unwrap().is_none(), "{name} interpreter");
        let expected = interp.get_regs().unwrap();
        assert_eq!(expected.rax, expected_rax, "{name}: reference RAX");
        assert_eq!(expected.rdx, expected_rdx, "{name}: reference RDX");

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        let ran_native = jit
            .jit_try_block()
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(
            ran_native,
            "{name} must enter the helper-backed native tier:\n{}",
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(actual.rax, expected.rax, "{name}: RAX");
        assert_eq!(actual.rdx, expected.rdx, "{name}: RDX");
        assert_eq!(actual.rbx, expected.rbx, "{name}: address base");
        assert_eq!(actual.r8, expected.r8, "{name}: unrelated GPR");
        assert_eq!(
            actual.rflags & DEFINED_FLAGS,
            expected.rflags & DEFINED_FLAGS,
            "{name}: defined CF/OF"
        );
        assert_eq!(actual.rip, expected.rip, "{name}: RIP");
        assert_eq!(
            jit_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
            memory,
            "{name}: source memory is unchanged"
        );
    }

    let code = [0x48, 0xF7, 0x23, 0xF4]; // mul qword [rbx]
    let (mut fault, _) = make_vcpu_mem(&code);
    let mut before = fault.get_regs().unwrap();
    before.rax = 0x0123_4567_89AB_CDEF;
    before.rdx = 0x1122_3344_5566_7788;
    before.rbx = MEM_SIZE + 0x1000;
    before.r8 = 0x8877_6655_4433_2211;
    before.rflags = 0xCD7;
    fault.set_regs(&before).unwrap();
    assert!(
        fault
            .jit_try_block()
            .expect("faulting widening memory MUL JIT"),
        "widening MUL must compile before its precise helper fault"
    );
    let after = fault.get_regs().unwrap();
    assert_eq!(after.rax, before.rax, "fault must preserve RAX");
    assert_eq!(after.rdx, before.rdx, "fault must preserve RDX");
    assert_eq!(after.rbx, before.rbx, "fault must preserve address base");
    assert_eq!(after.r8, before.r8, "fault must preserve unrelated GPRs");
    assert_eq!(after.rflags, before.rflags, "fault must preserve RFLAGS");
    assert_eq!(after.rip, LOAD_ADDR, "fault must restart current PC");
}

/// Unsigned DIV is admitted only through a guarded native lowering: the
/// divisor is staged before either implicit destination can change, zero and
/// quotient-overflow cases deopt at the current PC, and the successful path
/// matches the interpreter's deterministic policy of preserving RFLAGS.
#[test]
fn jit_guarded_unsigned_division_matches_all_widths_sources_and_faults() {
    const DATA: u64 = 0x20_0000;
    const INITIAL_RFLAGS: u64 = 0xCD7;

    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        rax: u64,
        rcx: u64,
        rdx: u64,
        rbx: u64,
        rsp: u64,
        rbp: u64,
        r16: u64,
        memory: Option<u64>,
        expected_rax: u64,
        expected_rdx: u64,
    }

    let cases = [
        Case {
            name: "DIV BL",
            instruction: &[0xF6, 0xF3],
            apx: false,
            rax: 0xA5A5_5A5A_1357_0120,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0x1122_3344_5566_7788,
            rbx: 0x0123_4567_89AB_CD10,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 0xA5A5_5A5A_1357_0012,
            expected_rdx: 0x1122_3344_5566_7788,
        },
        Case {
            name: "DIV CH",
            instruction: &[0xF6, 0xF5],
            apx: false,
            rax: 0xA5A5_5A5A_1357_0120,
            rcx: 0x8877_6655_4433_1011,
            rdx: 0x1122_3344_5566_7788,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 0xA5A5_5A5A_1357_0012,
            expected_rdx: 0x1122_3344_5566_7788,
        },
        Case {
            name: "DIV CX",
            instruction: &[0x66, 0xF7, 0xF1],
            apx: false,
            rax: 0xA5A5_5A5A_1357_0001,
            rcx: 3,
            rdx: 0x1122_3344_5566_0001,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 0xA5A5_5A5A_1357_5555,
            expected_rdx: 0x1122_3344_5566_0002,
        },
        Case {
            name: "DIV ECX",
            instruction: &[0xF7, 0xF1],
            apx: false,
            rax: 0xA5A5_5A5A_0000_0001,
            rcx: 3,
            rdx: 0x1122_3344_0000_0001,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 0x5555_5555,
            expected_rdx: 2,
        },
        Case {
            name: "DIV RCX",
            instruction: &[0x48, 0xF7, 0xF1],
            apx: false,
            rax: 1,
            rcx: 3,
            rdx: 1,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 0x5555_5555_5555_5555,
            expected_rdx: 2,
        },
        Case {
            name: "DIV RAX alias",
            instruction: &[0x48, 0xF7, 0xF0],
            apx: false,
            rax: 7,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 1,
            expected_rdx: 0,
        },
        Case {
            name: "DIV RBP state source",
            instruction: &[0x48, 0xF7, 0xF5],
            apx: false,
            rax: 22,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 3,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 7,
            expected_rdx: 1,
        },
        Case {
            name: "DIV RSP state source",
            instruction: &[0x48, 0xF7, 0xF4],
            apx: false,
            rax: 22,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 3,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 7,
            expected_rdx: 1,
        },
        Case {
            name: "APX NF DIV RBX",
            instruction: &[0x62, 0xF4, 0xFC, 0x0C, 0xF7, 0xF3],
            apx: true,
            rax: 22,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0,
            rbx: 3,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 7,
            expected_rdx: 1,
        },
        Case {
            name: "DIV R16 state source",
            instruction: &[0xD5, 0x18, 0xF7, 0xF0],
            apx: true,
            rax: 22,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 3,
            memory: None,
            expected_rax: 7,
            expected_rdx: 1,
        },
        Case {
            name: "DIV byte [RBX] helper source",
            instruction: &[0xF6, 0x33],
            apx: false,
            rax: 0xA5A5_5A5A_1357_0120,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0x1122_3344_5566_7788,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: Some(0x10),
            expected_rax: 0xA5A5_5A5A_1357_0012,
            expected_rdx: 0x1122_3344_5566_7788,
        },
        Case {
            name: "DIV word [RBX] helper source",
            instruction: &[0x66, 0xF7, 0x33],
            apx: false,
            rax: 0xA5A5_5A5A_1357_0001,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0x1122_3344_5566_0001,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: Some(3),
            expected_rax: 0xA5A5_5A5A_1357_5555,
            expected_rdx: 0x1122_3344_5566_0002,
        },
        Case {
            name: "DIV dword [RBX] helper source",
            instruction: &[0xF7, 0x33],
            apx: false,
            rax: 0xA5A5_5A5A_0000_0001,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0x1122_3344_0000_0001,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: Some(3),
            expected_rax: 0x5555_5555,
            expected_rdx: 2,
        },
        Case {
            name: "DIV qword [RBX] helper source",
            instruction: &[0x48, 0xF7, 0x33],
            apx: false,
            rax: 22,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: Some(3),
            expected_rax: 7,
            expected_rdx: 1,
        },
    ];

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, memory: &Arc<GuestMemoryMmap>| {
            if let Some(value) = case.memory {
                memory.write_obj(value, GuestAddress(DATA)).unwrap();
            }
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = case.rax;
            regs.rcx = case.rcx;
            regs.rdx = case.rdx;
            regs.rbx = case.rbx;
            regs.rsp = case.rsp;
            regs.rbp = case.rbp;
            regs.r8 = 0x0F0E_0D0C_0B0A_0908;
            regs.r16 = case.r16;
            regs.rflags = INITIAL_RFLAGS;
            vcpu.set_regs(&regs).unwrap();
            vcpu.set_apx_enabled(case.apx);
        };

        let (mut interp, interp_mem) = make_vcpu_mem(&code);
        setup(&mut interp, &interp_mem);
        let expected = if case.instruction.first() == Some(&0xD5) {
            // The classic instruction-step decoder does not currently expose
            // the REX2 Group-3 EGPR form. The lowerer byte-shape test proves
            // that D5 18 F7 F0 selects R16; use the explicit architectural
            // quotient/remainder oracle for this case.
            let mut expected = interp.get_regs().unwrap();
            expected.rax = case.expected_rax;
            expected.rdx = case.expected_rdx;
            expected.rip += case.instruction.len() as u64;
            expected
        } else {
            assert!(
                interp.step().unwrap().is_none(),
                "{} interpreter",
                case.name
            );
            interp.get_regs().unwrap()
        };
        assert_eq!(
            expected.rax, case.expected_rax,
            "{} reference RAX",
            case.name
        );
        assert_eq!(
            expected.rdx, case.expected_rdx,
            "{} reference RDX",
            case.name
        );

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the guarded native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        let actual_gprs = [
            actual.rax, actual.rcx, actual.rdx, actual.rbx, actual.rsp, actual.rbp, actual.rsi,
            actual.rdi, actual.r8, actual.r9, actual.r10, actual.r11, actual.r12, actual.r13,
            actual.r14, actual.r15, actual.r16, actual.r17, actual.r18, actual.r19, actual.r20,
            actual.r21, actual.r22, actual.r23, actual.r24, actual.r25, actual.r26, actual.r27,
            actual.r28, actual.r29, actual.r30, actual.r31,
        ];
        let expected_gprs = [
            expected.rax,
            expected.rcx,
            expected.rdx,
            expected.rbx,
            expected.rsp,
            expected.rbp,
            expected.rsi,
            expected.rdi,
            expected.r8,
            expected.r9,
            expected.r10,
            expected.r11,
            expected.r12,
            expected.r13,
            expected.r14,
            expected.r15,
            expected.r16,
            expected.r17,
            expected.r18,
            expected.r19,
            expected.r20,
            expected.r21,
            expected.r22,
            expected.r23,
            expected.r24,
            expected.r25,
            expected.r26,
            expected.r27,
            expected.r28,
            expected.r29,
            expected.r30,
            expected.r31,
        ];
        for (index, (actual, expected)) in actual_gprs.into_iter().zip(expected_gprs).enumerate() {
            assert_eq!(actual, expected, "{} GPR index {index}", case.name);
        }
        assert_eq!(
            actual.rflags, expected.rflags,
            "{} RFLAGS policy",
            case.name
        );
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
        if let Some(value) = case.memory {
            assert_eq!(
                jit_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
                value,
                "{} source memory",
                case.name
            );
        }
    }

    for (name, instruction, rax, rcx, rdx, rbx) in [
        (
            "zero divisor",
            &[0x48, 0xF7, 0xF1, 0xF4][..],
            0x0123_4567_89AB_CDEF,
            0,
            0,
            0x8877_6655_4433_2211,
        ),
        (
            "quotient overflow",
            &[0x48, 0xF7, 0xF1, 0xF4][..],
            0x0123_4567_89AB_CDEF,
            3,
            3,
            0x8877_6655_4433_2211,
        ),
        (
            "RDX divisor alias overflow",
            &[0x48, 0xF7, 0xF2, 0xF4][..],
            0x0123_4567_89AB_CDEF,
            0x7766_5544_3322_1100,
            3,
            0x8877_6655_4433_2211,
        ),
        (
            "AH divisor alias overflow",
            &[0xF6, 0xF4, 0xF4][..],
            0x0123_4567_89AB_0301,
            0x7766_5544_3322_1100,
            0x1122_3344_5566_7788,
            0x8877_6655_4433_2211,
        ),
        (
            "memory load fault",
            &[0x48, 0xF7, 0x33, 0xF4][..],
            0x0123_4567_89AB_CDEF,
            0x7766_5544_3322_1100,
            0,
            MEM_SIZE + 0x1000,
        ),
    ] {
        let (mut fault, _) = make_vcpu_mem(instruction);
        let mut before = fault.get_regs().unwrap();
        before.rax = rax;
        before.rcx = rcx;
        before.rdx = rdx;
        before.rbx = rbx;
        before.r8 = 0x0F0E_0D0C_0B0A_0908;
        before.rflags = INITIAL_RFLAGS;
        fault.set_regs(&before).unwrap();
        assert!(
            fault
                .jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error:?}")),
            "{name} must compile before precise deoptimization"
        );
        let after = fault.get_regs().unwrap();
        assert_eq!(after.rax, before.rax, "{name}: RAX");
        assert_eq!(after.rcx, before.rcx, "{name}: RCX");
        assert_eq!(after.rdx, before.rdx, "{name}: RDX");
        assert_eq!(after.rbx, before.rbx, "{name}: RBX");
        assert_eq!(after.r8, before.r8, "{name}: unrelated GPR");
        assert_eq!(after.rsp, before.rsp, "{name}: RSP");
        assert_eq!(after.rbp, before.rbp, "{name}: RBP");
        assert_eq!(after.rflags, before.rflags, "{name}: RFLAGS");
        assert_eq!(after.rip, LOAD_ADDR, "{name}: restart PC");
    }

    for (name, divisor, high) in [
        ("mapped memory zero divisor", 0u64, 0u64),
        ("mapped memory quotient overflow", 3, 3),
    ] {
        let code = [0x48, 0xF7, 0x33, 0xF4]; // div qword [rbx]; hlt
        let (mut fault, memory) = make_vcpu_mem(&code);
        memory.write_obj(divisor, GuestAddress(DATA)).unwrap();
        let mut before = fault.get_regs().unwrap();
        before.rax = 0x0123_4567_89AB_CDEF;
        before.rdx = high;
        before.rbx = DATA;
        before.r8 = 0x0F0E_0D0C_0B0A_0908;
        before.rflags = INITIAL_RFLAGS;
        fault.set_regs(&before).unwrap();
        assert!(
            fault
                .jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error:?}")),
            "{name} must compile through the helper before guarded deoptimization"
        );
        let after = fault.get_regs().unwrap();
        assert_eq!(after.rax, before.rax, "{name}: RAX");
        assert_eq!(after.rdx, before.rdx, "{name}: RDX");
        assert_eq!(after.rbx, before.rbx, "{name}: address base");
        assert_eq!(after.r8, before.r8, "{name}: unrelated GPR");
        assert_eq!(after.rflags, before.rflags, "{name}: RFLAGS");
        assert_eq!(after.rip, LOAD_ADDR, "{name}: restart PC");
        assert_eq!(
            memory.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
            divisor,
            "{name}: source memory"
        );
    }

    // Earlier instructions in the same region are already architectural when
    // the guarded DIV deopts; only the faulting instruction is uncommitted.
    let prior_commit_code = [
        0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,0x12345678
        0xBA, 0x00, 0x00, 0x00, 0x00, // mov edx,0
        0x48, 0xF7, 0xF1, // div rcx (zero divisor)
        0x41, 0xB8, 0x01, 0x00, 0x00, 0x00, // mov r8d,1 (must not execute)
        0xF4,
    ];
    let (mut prior_commit, _) = make_vcpu_mem(&prior_commit_code);
    let mut before = prior_commit.get_regs().unwrap();
    before.rax = 0xFFFF_FFFF_FFFF_FFFF;
    before.rcx = 0;
    before.rdx = 0xFFFF_FFFF_FFFF_FFFF;
    before.r8 = 0x0F0E_0D0C_0B0A_0908;
    before.rflags = INITIAL_RFLAGS;
    prior_commit.set_regs(&before).unwrap();
    assert!(
        prior_commit
            .jit_try_block()
            .expect("guarded DIV after prior native writes"),
        "the region must compile before the guarded fault"
    );
    let after = prior_commit.get_regs().unwrap();
    assert_eq!(after.rax, 0x1234_5678, "prior EAX write must commit");
    assert_eq!(after.rdx, 0, "prior EDX write must commit");
    assert_eq!(after.rcx, 0, "divisor must remain unchanged");
    assert_eq!(after.r8, before.r8, "post-DIV write must not execute");
    assert_eq!(
        after.rflags, before.rflags,
        "MOVs and DIV fault preserve flags"
    );
    assert_eq!(after.rip, LOAD_ADDR + 10, "restart at the DIV instruction");
}

/// Signed IDIV uses an exact magnitude threshold before native execution. The
/// successful path covers every operand width, all quotient sign combinations,
/// 128-bit dividend magnitudes, aliases, APX sources, and helper-backed memory;
/// every architected #DE condition deoptimizes without committing IDIV.
#[test]
fn jit_guarded_signed_division_matches_all_widths_boundaries_and_faults() {
    const DATA: u64 = 0x20_0000;
    const INITIAL_RFLAGS: u64 = 0xCD7;

    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        rax: u64,
        rcx: u64,
        rdx: u64,
        rbx: u64,
        rsp: u64,
        rbp: u64,
        r16: u64,
        memory: Option<u64>,
        expected_rax: u64,
        expected_rdx: u64,
    }

    let cases = [
        Case {
            name: "IDIV BL negative dividend",
            instruction: &[0xF6, 0xFB],
            apx: false,
            rax: 0xA5A5_5A5A_1357_FFDF,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0x1122_3344_5566_7788,
            rbx: 5,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 0xA5A5_5A5A_1357_FDFA,
            expected_rdx: 0x1122_3344_5566_7788,
        },
        Case {
            name: "IDIV CH negative divisor",
            instruction: &[0xF6, 0xFD],
            apx: false,
            rax: 0xA5A5_5A5A_1357_0021,
            rcx: 0x8877_6655_4433_FB11,
            rdx: 0x1122_3344_5566_7788,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 0xA5A5_5A5A_1357_03FA,
            expected_rdx: 0x1122_3344_5566_7788,
        },
        Case {
            name: "minimum byte quotient boundary",
            instruction: &[0xF6, 0xFB],
            apx: false,
            rax: 0xA5A5_5A5A_1357_FF80,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0x1122_3344_5566_7788,
            rbx: 1,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 0xA5A5_5A5A_1357_0080,
            expected_rdx: 0x1122_3344_5566_7788,
        },
        Case {
            name: "IDIV CX negative by positive",
            instruction: &[0x66, 0xF7, 0xF9],
            apx: false,
            rax: 0xA5A5_5A5A_1357_7960,
            rcx: 300,
            rdx: 0x1122_3344_5566_FFFE,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 0xA5A5_5A5A_1357_FEB3,
            expected_rdx: 0x1122_3344_5566_FF9C,
        },
        Case {
            name: "IDIV ECX negative by negative",
            instruction: &[0xF7, 0xF9],
            apx: false,
            rax: 0xA5A5_5A5A_ABF4_1C00,
            rcx: (-3000i64) as u64,
            rdx: 0x1122_3344_FFFF_FFFD,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 0x0032_DCD5,
            expected_rdx: 0xFFFF_FC18,
        },
        Case {
            name: "IDIV RCX positive 128-bit dividend",
            instruction: &[0x48, 0xF7, 0xF9],
            apx: false,
            rax: 0,
            rcx: 3,
            rdx: 1,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 0x5555_5555_5555_5555,
            expected_rdx: 1,
        },
        Case {
            name: "IDIV RCX negative 128-bit dividend low zero",
            instruction: &[0x48, 0xF7, 0xF9],
            apx: false,
            rax: 0,
            rcx: 3,
            rdx: u64::MAX,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 0xAAAA_AAAA_AAAA_AAAB,
            expected_rdx: u64::MAX,
        },
        Case {
            name: "IDIV RCX negative 128-bit dividend low nonzero",
            instruction: &[0x48, 0xF7, 0xF9],
            apx: false,
            rax: u64::MAX,
            rcx: 3,
            rdx: u64::MAX - 1,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 0xAAAA_AAAA_AAAA_AAAB,
            expected_rdx: u64::MAX - 1,
        },
        Case {
            name: "IDIV RAX alias",
            instruction: &[0x48, 0xF7, 0xF8],
            apx: false,
            rax: 7,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 1,
            expected_rdx: 0,
        },
        Case {
            name: "IDIV RDX alias",
            instruction: &[0x48, 0xF7, 0xFA],
            apx: false,
            rax: (-100i64) as u64,
            rcx: 0x8877_6655_4433_2211,
            rdx: u64::MAX,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 100,
            expected_rdx: 0,
        },
        Case {
            name: "IDIV RBP state source",
            instruction: &[0x48, 0xF7, 0xFD],
            apx: false,
            rax: 22,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: (-3i64) as u64,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: (-7i64) as u64,
            expected_rdx: 1,
        },
        Case {
            name: "IDIV RSP state source",
            instruction: &[0x48, 0xF7, 0xFC],
            apx: false,
            rax: 22,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: (-3i64) as u64,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: (-7i64) as u64,
            expected_rdx: 1,
        },
        Case {
            name: "APX NF IDIV RBX",
            instruction: &[0x62, 0xF4, 0xFC, 0x0C, 0xF7, 0xFB],
            apx: true,
            rax: 22,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0,
            rbx: (-3i64) as u64,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: (-7i64) as u64,
            expected_rdx: 1,
        },
        Case {
            name: "IDIV R16 state source",
            instruction: &[0xD5, 0x18, 0xF7, 0xF8],
            apx: true,
            rax: 22,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: (-3i64) as u64,
            memory: None,
            expected_rax: (-7i64) as u64,
            expected_rdx: 1,
        },
        Case {
            name: "IDIV byte [RBX] helper source",
            instruction: &[0xF6, 0x3B],
            apx: false,
            rax: 0xA5A5_5A5A_1357_0021,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0x1122_3344_5566_7788,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: Some(0xFB),
            expected_rax: 0xA5A5_5A5A_1357_03FA,
            expected_rdx: 0x1122_3344_5566_7788,
        },
        Case {
            name: "IDIV word [RBX] helper source",
            instruction: &[0x66, 0xF7, 0x3B],
            apx: false,
            rax: 0xA5A5_5A5A_1357_86A0,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0x1122_3344_5566_0001,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: Some(0xFED4),
            expected_rax: 0xA5A5_5A5A_1357_FEB3,
            expected_rdx: 0x1122_3344_5566_0064,
        },
        Case {
            name: "IDIV dword [RBX] helper source",
            instruction: &[0xF7, 0x3B],
            apx: false,
            rax: 0xA5A5_5A5A_ABF4_1C00,
            rcx: 0x8877_6655_4433_2211,
            rdx: 0x1122_3344_FFFF_FFFD,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: Some(0xFFFF_F448),
            expected_rax: 0x0032_DCD5,
            expected_rdx: 0xFFFF_FC18,
        },
        Case {
            name: "IDIV qword [RBX] helper source",
            instruction: &[0x48, 0xF7, 0x3B],
            apx: false,
            rax: (-100i64) as u64,
            rcx: 0x8877_6655_4433_2211,
            rdx: u64::MAX,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: Some((-7i64) as u64),
            expected_rax: 14,
            expected_rdx: (-2i64) as u64,
        },
        Case {
            name: "minimum quotient boundary",
            instruction: &[0x48, 0xF7, 0xF9],
            apx: false,
            rax: i64::MIN as u64,
            rcx: 1,
            rdx: u64::MAX,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: i64::MIN as u64,
            expected_rdx: 0,
        },
        Case {
            name: "maximum quotient boundary",
            instruction: &[0x48, 0xF7, 0xF9],
            apx: false,
            rax: i64::MAX as u64,
            rcx: 1,
            rdx: 0,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: i64::MAX as u64,
            expected_rdx: 0,
        },
        Case {
            name: "minimum divisor boundary",
            instruction: &[0x48, 0xF7, 0xF9],
            apx: false,
            rax: i64::MIN as u64,
            rcx: i64::MIN as u64,
            rdx: u64::MAX,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            r16: 0xCAFE_BABE_DEAD_BEEF,
            memory: None,
            expected_rax: 1,
            expected_rdx: 0,
        },
    ];

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, memory: &Arc<GuestMemoryMmap>| {
            if let Some(value) = case.memory {
                memory.write_obj(value, GuestAddress(DATA)).unwrap();
            }
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = case.rax;
            regs.rcx = case.rcx;
            regs.rdx = case.rdx;
            regs.rbx = case.rbx;
            regs.rsp = case.rsp;
            regs.rbp = case.rbp;
            regs.r8 = 0x0F0E_0D0C_0B0A_0908;
            regs.r16 = case.r16;
            regs.rflags = INITIAL_RFLAGS;
            vcpu.set_regs(&regs).unwrap();
            vcpu.set_apx_enabled(case.apx);
        };

        let (mut interp, interp_mem) = make_vcpu_mem(&code);
        setup(&mut interp, &interp_mem);
        let expected = if case.instruction.first() == Some(&0xD5) {
            // The classic step decoder does not expose REX2 Group-3 EGPR.
            // The lowerer byte-shape test proves that this form selects R16.
            let mut expected = interp.get_regs().unwrap();
            expected.rax = case.expected_rax;
            expected.rdx = case.expected_rdx;
            expected.rip += case.instruction.len() as u64;
            expected
        } else {
            assert!(
                interp.step().unwrap().is_none(),
                "{} interpreter",
                case.name
            );
            interp.get_regs().unwrap()
        };
        assert_eq!(
            expected.rax, case.expected_rax,
            "{} reference RAX",
            case.name
        );
        assert_eq!(
            expected.rdx, case.expected_rdx,
            "{} reference RDX",
            case.name
        );

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the guarded native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        let actual_gprs = [
            actual.rax, actual.rcx, actual.rdx, actual.rbx, actual.rsp, actual.rbp, actual.rsi,
            actual.rdi, actual.r8, actual.r9, actual.r10, actual.r11, actual.r12, actual.r13,
            actual.r14, actual.r15, actual.r16, actual.r17, actual.r18, actual.r19, actual.r20,
            actual.r21, actual.r22, actual.r23, actual.r24, actual.r25, actual.r26, actual.r27,
            actual.r28, actual.r29, actual.r30, actual.r31,
        ];
        let expected_gprs = [
            expected.rax,
            expected.rcx,
            expected.rdx,
            expected.rbx,
            expected.rsp,
            expected.rbp,
            expected.rsi,
            expected.rdi,
            expected.r8,
            expected.r9,
            expected.r10,
            expected.r11,
            expected.r12,
            expected.r13,
            expected.r14,
            expected.r15,
            expected.r16,
            expected.r17,
            expected.r18,
            expected.r19,
            expected.r20,
            expected.r21,
            expected.r22,
            expected.r23,
            expected.r24,
            expected.r25,
            expected.r26,
            expected.r27,
            expected.r28,
            expected.r29,
            expected.r30,
            expected.r31,
        ];
        for (index, (actual, expected)) in actual_gprs.into_iter().zip(expected_gprs).enumerate() {
            assert_eq!(actual, expected, "{} GPR index {index}", case.name);
        }
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
        if let Some(value) = case.memory {
            assert_eq!(
                jit_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
                value,
                "{} source memory",
                case.name
            );
        }
    }

    for (name, instruction, rax, rcx, rdx, rbx) in [
        (
            "zero divisor",
            &[0x48, 0xF7, 0xF9, 0xF4][..],
            0x0123_4567_89AB_CDEF,
            0,
            0,
            0x8877_6655_4433_2211,
        ),
        (
            "positive quotient overflow",
            &[0x48, 0xF7, 0xF9, 0xF4][..],
            0x8000_0000_0000_0000,
            1,
            0,
            0x8877_6655_4433_2211,
        ),
        (
            "negative quotient overflow",
            &[0x48, 0xF7, 0xF9, 0xF4][..],
            0x7FFF_FFFF_FFFF_FFFF,
            1,
            u64::MAX,
            0x8877_6655_4433_2211,
        ),
        (
            "minimum dividend divided by minus one",
            &[0x48, 0xF7, 0xF9, 0xF4][..],
            i64::MIN as u64,
            u64::MAX,
            u64::MAX,
            0x8877_6655_4433_2211,
        ),
        (
            "RDX divisor alias overflow",
            &[0x48, 0xF7, 0xFA, 0xF4][..],
            i64::MIN as u64,
            0x7766_5544_3322_1100,
            u64::MAX,
            0x8877_6655_4433_2211,
        ),
        (
            "AH divisor alias overflow",
            &[0xF6, 0xFC, 0xF4][..],
            0x0123_4567_89AB_8000,
            0x7766_5544_3322_1100,
            0x1122_3344_5566_7788,
            0x8877_6655_4433_2211,
        ),
        (
            "negative byte quotient overflow",
            &[0xF6, 0xFB, 0xF4][..],
            0x0123_4567_89AB_FF7F,
            0x7766_5544_3322_1100,
            0x1122_3344_5566_7788,
            1,
        ),
        (
            "memory load fault",
            &[0x48, 0xF7, 0x3B, 0xF4][..],
            0x0123_4567_89AB_CDEF,
            0x7766_5544_3322_1100,
            0,
            MEM_SIZE + 0x1000,
        ),
    ] {
        let (mut fault, _) = make_vcpu_mem(instruction);
        let mut before = fault.get_regs().unwrap();
        before.rax = rax;
        before.rcx = rcx;
        before.rdx = rdx;
        before.rbx = rbx;
        before.r8 = 0x0F0E_0D0C_0B0A_0908;
        before.rflags = INITIAL_RFLAGS;
        fault.set_regs(&before).unwrap();
        assert!(
            fault
                .jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error:?}")),
            "{name} must compile before precise deoptimization"
        );
        let after = fault.get_regs().unwrap();
        assert_eq!(after.rax, before.rax, "{name}: RAX");
        assert_eq!(after.rcx, before.rcx, "{name}: RCX");
        assert_eq!(after.rdx, before.rdx, "{name}: RDX");
        assert_eq!(after.rbx, before.rbx, "{name}: RBX");
        assert_eq!(after.r8, before.r8, "{name}: unrelated GPR");
        assert_eq!(after.rsp, before.rsp, "{name}: RSP");
        assert_eq!(after.rbp, before.rbp, "{name}: RBP");
        assert_eq!(after.rflags, before.rflags, "{name}: RFLAGS");
        assert_eq!(after.rip, LOAD_ADDR, "{name}: restart PC");
    }

    for (name, divisor, rax, rdx) in [
        ("mapped memory zero divisor", 0u64, 1u64, 0u64),
        (
            "mapped memory positive overflow",
            1,
            0x8000_0000_0000_0000,
            0,
        ),
        (
            "mapped memory negative overflow",
            1,
            0x7FFF_FFFF_FFFF_FFFF,
            u64::MAX,
        ),
    ] {
        let code = [0x48, 0xF7, 0x3B, 0xF4]; // idiv qword [rbx]; hlt
        let (mut fault, memory) = make_vcpu_mem(&code);
        memory.write_obj(divisor, GuestAddress(DATA)).unwrap();
        let mut before = fault.get_regs().unwrap();
        before.rax = rax;
        before.rdx = rdx;
        before.rbx = DATA;
        before.r8 = 0x0F0E_0D0C_0B0A_0908;
        before.rflags = INITIAL_RFLAGS;
        fault.set_regs(&before).unwrap();
        assert!(
            fault
                .jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error:?}")),
            "{name} must compile through the helper before guarded deoptimization"
        );
        let after = fault.get_regs().unwrap();
        assert_eq!(after.rax, before.rax, "{name}: RAX");
        assert_eq!(after.rdx, before.rdx, "{name}: RDX");
        assert_eq!(after.rbx, before.rbx, "{name}: address base");
        assert_eq!(after.r8, before.r8, "{name}: unrelated GPR");
        assert_eq!(after.rflags, before.rflags, "{name}: RFLAGS");
        assert_eq!(after.rip, LOAD_ADDR, "{name}: restart PC");
        assert_eq!(
            memory.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
            divisor,
            "{name}: source memory"
        );
    }

    // Earlier writes are committed, while the current IDIV and later write
    // remain uncommitted when the zero-divisor guard deoptimizes.
    let prior_commit_code = [
        0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,0x12345678
        0xBA, 0x00, 0x00, 0x00, 0x00, // mov edx,0
        0x48, 0xF7, 0xF9, // idiv rcx (zero divisor)
        0x41, 0xB8, 0x01, 0x00, 0x00, 0x00, // mov r8d,1 (must not execute)
        0xF4,
    ];
    let (mut prior_commit, _) = make_vcpu_mem(&prior_commit_code);
    let mut before = prior_commit.get_regs().unwrap();
    before.rax = u64::MAX;
    before.rcx = 0;
    before.rdx = u64::MAX;
    before.r8 = 0x0F0E_0D0C_0B0A_0908;
    before.rflags = INITIAL_RFLAGS;
    prior_commit.set_regs(&before).unwrap();
    assert!(
        prior_commit
            .jit_try_block()
            .expect("guarded IDIV after prior native writes"),
        "the region must compile before the guarded signed fault"
    );
    let after = prior_commit.get_regs().unwrap();
    assert_eq!(after.rax, 0x1234_5678, "prior EAX write must commit");
    assert_eq!(after.rdx, 0, "prior EDX write must commit");
    assert_eq!(after.rcx, 0, "divisor must remain unchanged");
    assert_eq!(after.r8, before.r8, "post-IDIV write must not execute");
    assert_eq!(
        after.rflags, before.rflags,
        "MOVs and IDIV fault preserve flags"
    );
    assert_eq!(after.rip, LOAD_ADDR + 10, "restart at the IDIV instruction");
}

/// Memory-source CMOVcc performs its memory read before evaluating the
/// condition. The JIT must therefore call the helper on both true and false
/// paths, preserve exact destination-width semantics, and fault before commit.
#[test]
fn jit_memory_cmov_matches_widths_addresses_conditions_and_faults() {
    const DATA: u64 = 0x20_0000;
    const FS_BASE: u64 = 0x30_0000;
    const FLAGS_ZF_SET: u64 = 0xCD7;
    const FLAGS_ZF_CLEAR: u64 = 0xC97;

    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        rflags: u64,
        source_address: u64,
        source: u64,
        fs_base: u64,
        rax: u64,
        rdx: u64,
        rbx: u64,
        rsp: u64,
        rbp: u64,
        destination_index: usize,
        expected_destination: u64,
    }

    let cases = [
        Case {
            name: "CMOVNE AX,word [RBX] true partial destination",
            instruction: &[0x66, 0x0F, 0x45, 0x03],
            apx: false,
            rflags: FLAGS_ZF_CLEAR,
            source_address: DATA,
            source: 0xFEDC,
            fs_base: 0,
            rax: 0xA5A5_5A5A_1357_2468,
            rdx: 0x1122_3344_5566_7788,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            destination_index: 0,
            expected_destination: 0xA5A5_5A5A_1357_FEDC,
        },
        Case {
            name: "CMOVNE EAX,dword [RBX] false zeroes upper dword",
            instruction: &[0x0F, 0x45, 0x03],
            apx: false,
            rflags: FLAGS_ZF_SET,
            source_address: DATA,
            source: 0x8000_0001,
            fs_base: 0,
            rax: 0xA5A5_5A5A_1357_2468,
            rdx: 0x1122_3344_5566_7788,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            destination_index: 0,
            expected_destination: 0x1357_2468,
        },
        Case {
            name: "CMOVE ECX,dword [RBX+3] true zeroes upper dword",
            instruction: &[0x0F, 0x44, 0x4B, 0x03],
            apx: false,
            rflags: FLAGS_ZF_SET,
            source_address: DATA + 3,
            source: 0x8000_0001,
            fs_base: 0,
            rax: 0xA5A5_5A5A_1357_2468,
            rdx: 0x1122_3344_5566_7788,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            destination_index: 1,
            expected_destination: 0x8000_0001,
        },
        Case {
            name: "CMOVL R8,qword [RBX+RDX*2+6] true SIB",
            instruction: &[0x4C, 0x0F, 0x4C, 0x44, 0x53, 0x06],
            apx: false,
            rflags: 0x4D7,
            source_address: DATA + 10,
            source: 0x8877_6655_4433_2211,
            fs_base: 0,
            rax: 0xA5A5_5A5A_1357_2468,
            rdx: 2,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            destination_index: 8,
            expected_destination: 0x8877_6655_4433_2211,
        },
        Case {
            name: "CMOVGE R9,qword FS:[RBX+2] true segment address",
            instruction: &[0x64, 0x4C, 0x0F, 0x4D, 0x4B, 0x02],
            apx: false,
            rflags: FLAGS_ZF_SET,
            source_address: FS_BASE + 0x102,
            source: 0x1020_3040_5060_7080,
            fs_base: FS_BASE,
            rax: 0xA5A5_5A5A_1357_2468,
            rdx: 0x1122_3344_5566_7788,
            rbx: 0x100,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            destination_index: 9,
            expected_destination: 0x1020_3040_5060_7080,
        },
        Case {
            name: "CMOVNE RAX,qword [RAX] true address alias",
            instruction: &[0x48, 0x0F, 0x45, 0x00],
            apx: false,
            rflags: FLAGS_ZF_CLEAR,
            source_address: DATA,
            source: 0x0F1E_2D3C_4B5A_6978,
            fs_base: 0,
            rax: DATA,
            rdx: 0x1122_3344_5566_7788,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            destination_index: 0,
            expected_destination: 0x0F1E_2D3C_4B5A_6978,
        },
        Case {
            name: "CMOVNE SP,word [RBX] true state-backed partial destination",
            instruction: &[0x66, 0x0F, 0x45, 0x23],
            apx: false,
            rflags: FLAGS_ZF_CLEAR,
            source_address: DATA,
            source: 0xBEEF,
            fs_base: 0,
            rax: 0xA5A5_5A5A_1357_2468,
            rdx: 0x1122_3344_5566_7788,
            rbx: DATA,
            rsp: 0x1234_5678_9ABC_DEF0,
            rbp: 0x19_0000,
            destination_index: 4,
            expected_destination: 0x1234_5678_9ABC_BEEF,
        },
        Case {
            name: "CMOVNE SP,word [RBX] false preserves complete destination",
            instruction: &[0x66, 0x0F, 0x45, 0x23],
            apx: false,
            rflags: FLAGS_ZF_SET,
            source_address: DATA,
            source: 0xBEEF,
            fs_base: 0,
            rax: 0xA5A5_5A5A_1357_2468,
            rdx: 0x1122_3344_5566_7788,
            rbx: DATA,
            rsp: 0x1234_5678_9ABC_DEF0,
            rbp: 0x19_0000,
            destination_index: 4,
            expected_destination: 0x1234_5678_9ABC_DEF0,
        },
        Case {
            name: "CMOVNE EBP,dword [RBX] false state-backed destination",
            instruction: &[0x0F, 0x45, 0x2B],
            apx: false,
            rflags: FLAGS_ZF_SET,
            source_address: DATA,
            source: 0xDEAD_BEEF,
            fs_base: 0,
            rax: 0xA5A5_5A5A_1357_2468,
            rdx: 0x1122_3344_5566_7788,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x1234_5678_9ABC_DEF0,
            destination_index: 5,
            expected_destination: 0x9ABC_DEF0,
        },
        Case {
            name: "CMOVE EBP,dword [RBX] true state-backed destination",
            instruction: &[0x0F, 0x44, 0x2B],
            apx: false,
            rflags: FLAGS_ZF_SET,
            source_address: DATA,
            source: 0xDEAD_BEEF,
            fs_base: 0,
            rax: 0xA5A5_5A5A_1357_2468,
            rdx: 0x1122_3344_5566_7788,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x1234_5678_9ABC_DEF0,
            destination_index: 5,
            expected_destination: 0xDEAD_BEEF,
        },
        Case {
            name: "REX2 CMOVNE R16,qword [RBX] true state-backed EGPR",
            instruction: &[0xD5, 0xC8, 0x45, 0x03],
            apx: true,
            rflags: FLAGS_ZF_CLEAR,
            source_address: DATA,
            source: 0xCAF0_BABE_1234_5678,
            fs_base: 0,
            rax: 0xA5A5_5A5A_1357_2468,
            rdx: 0x1122_3344_5566_7788,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            destination_index: 16,
            expected_destination: 0xCAF0_BABE_1234_5678,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, memory: &Arc<GuestMemoryMmap>| {
            memory
                .write_obj(case.source, GuestAddress(case.source_address))
                .unwrap();
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = case.rax;
            regs.rcx = 0x8877_6655_4433_2211;
            regs.rdx = case.rdx;
            regs.rbx = case.rbx;
            regs.rsp = case.rsp;
            regs.rbp = case.rbp;
            regs.rsi = 0x99AA_BBCC_DDEE_FF00;
            regs.rdi = 0x0F1E_2D3C_4B5A_6978;
            regs.r8 = 0x0102_0304_0506_0708;
            regs.r9 = 0x1112_1314_1516_1718;
            regs.r10 = 0x2122_2324_2526_2728;
            regs.r16 = 0xA1A2_A3A4_A5A6_A7A8;
            regs.r17 = 0xB1B2_B3B4_B5B6_B7B8;
            regs.r31 = 0xF1F2_F3F4_F5F6_F7F8;
            regs.rflags = case.rflags;
            vcpu.set_regs(&regs).unwrap();
            if case.fs_base != 0 {
                let mut sregs = vcpu.get_sregs().unwrap();
                sregs.fs.base = case.fs_base;
                vcpu.set_sregs(&sregs).unwrap();
            }
            vcpu.set_apx_enabled(case.apx);
        };

        let (mut interp, interp_mem) = make_vcpu_mem(&code);
        setup(&mut interp, &interp_mem);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        assert_eq!(
            gprs(&expected)[case.destination_index],
            case.expected_destination,
            "{} reference destination",
            case.name
        );

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the helper-backed native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
        assert_eq!(
            jit_mem
                .read_obj::<u64>(GuestAddress(case.source_address))
                .unwrap(),
            case.source,
            "{} source memory",
            case.name
        );
    }

    // The false condition does not suppress the architectural memory read: a
    // prior write commits, while the faulting CMOV and later write do not.
    let fault_code = [
        0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,0x12345678
        0x48, 0x0F, 0x45, 0x0B, // cmovne rcx,qword [rbx] (ZF=1, false)
        0x41, 0xB8, 0x01, 0x00, 0x00, 0x00, // mov r8d,1 (must not execute)
        0xF4,
    ];
    let (mut fault, _) = make_vcpu_mem(&fault_code);
    let mut before = fault.get_regs().unwrap();
    before.rax = u64::MAX;
    before.rcx = 0x8877_6655_4433_2211;
    before.rbx = MEM_SIZE + 0x1000;
    before.r8 = 0x0F0E_0D0C_0B0A_0908;
    before.rflags = FLAGS_ZF_SET;
    fault.set_regs(&before).unwrap();
    assert!(
        fault
            .jit_try_block()
            .expect("faulting helper-backed false CMOV JIT"),
        "memory CMOV must compile before its unconditional helper fault"
    );
    let after = fault.get_regs().unwrap();
    assert_eq!(after.rax, 0x1234_5678, "prior EAX write must commit");
    assert_eq!(after.rcx, before.rcx, "faulting CMOV must not commit");
    assert_eq!(after.rbx, before.rbx, "address base must remain unchanged");
    assert_eq!(after.r8, before.r8, "post-fault write must not execute");
    assert_eq!(after.rflags, before.rflags, "CMOV fault preserves RFLAGS");
    assert_eq!(after.rip, LOAD_ADDR + 5, "fault restarts at CMOV");
}

/// Memory-source MOVZX/MOVSX/MOVSXD pairs must remain in the native tier while
/// preserving their destination-width write semantics. The helper snapshots
/// the effective address before an aliased destination commits, and a helper
/// fault restarts the current instruction without committing its extension.
#[test]
fn jit_memory_extensions_match_all_widths_addresses_and_faults() {
    const DATA: u64 = 0x20_0000;
    const FS_BASE: u64 = 0x30_0000;
    const INITIAL_RFLAGS: u64 = 0xCD7;

    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        source_address: u64,
        source: u64,
        fs_base: u64,
        rax: u64,
        rcx: u64,
        rbx: u64,
        rsp: u64,
        rbp: u64,
        destination_index: usize,
        expected_destination: u64,
    }

    let cases = [
        Case {
            name: "MOVZX AX,byte [RBX] partial destination",
            instruction: &[0x66, 0x0F, 0xB6, 0x03],
            apx: false,
            source_address: DATA,
            source: 0xFE,
            fs_base: 0,
            rax: 0xA5A5_5A5A_1357_2468,
            rcx: 0x8877_6655_4433_2211,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            destination_index: 0,
            expected_destination: 0xA5A5_5A5A_1357_00FE,
        },
        Case {
            name: "MOVSX ECX,byte [RBX+3]",
            instruction: &[0x0F, 0xBE, 0x4B, 0x03],
            apx: false,
            source_address: DATA + 3,
            source: 0x80,
            fs_base: 0,
            rax: 0xA5A5_5A5A_1357_2468,
            rcx: 0x8877_6655_4433_2211,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            destination_index: 1,
            expected_destination: 0x0000_0000_FFFF_FF80,
        },
        Case {
            name: "MOVZX R8,word [RBX+RCX*2+6]",
            instruction: &[0x4C, 0x0F, 0xB7, 0x44, 0x4B, 0x06],
            apx: false,
            source_address: DATA + 10,
            source: 0xFEDC,
            fs_base: 0,
            rax: 0xA5A5_5A5A_1357_2468,
            rcx: 2,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            destination_index: 8,
            expected_destination: 0xFEDC,
        },
        Case {
            name: "MOVSX R9,word FS:[RBX+2]",
            instruction: &[0x64, 0x4C, 0x0F, 0xBF, 0x4B, 0x02],
            apx: false,
            source_address: FS_BASE + 0x102,
            source: 0x8001,
            fs_base: FS_BASE,
            rax: 0xA5A5_5A5A_1357_2468,
            rcx: 0x8877_6655_4433_2211,
            rbx: 0x100,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            destination_index: 9,
            expected_destination: 0xFFFF_FFFF_FFFF_8001,
        },
        Case {
            name: "MOVSXD R10,dword [RAX]",
            instruction: &[0x4C, 0x63, 0x10],
            apx: false,
            source_address: DATA,
            source: 0x8000_0001,
            fs_base: 0,
            rax: DATA,
            rcx: 0x8877_6655_4433_2211,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            destination_index: 10,
            expected_destination: 0xFFFF_FFFF_8000_0001,
        },
        Case {
            name: "MOVSX RAX,byte [RAX] address alias",
            instruction: &[0x48, 0x0F, 0xBE, 0x00],
            apx: false,
            source_address: DATA,
            source: 0x81,
            fs_base: 0,
            rax: DATA,
            rcx: 0x8877_6655_4433_2211,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            destination_index: 0,
            expected_destination: 0xFFFF_FFFF_FFFF_FF81,
        },
        Case {
            name: "MOVZX SP,byte [RBX] state-backed partial destination",
            instruction: &[0x66, 0x0F, 0xB6, 0x23],
            apx: false,
            source_address: DATA,
            source: 0x7F,
            fs_base: 0,
            rax: 0xA5A5_5A5A_1357_2468,
            rcx: 0x8877_6655_4433_2211,
            rbx: DATA,
            rsp: 0x1234_5678_9ABC_DEF0,
            rbp: 0x19_0000,
            destination_index: 4,
            expected_destination: 0x1234_5678_9ABC_007F,
        },
        Case {
            name: "MOVSX EBP,word [RBX] state-backed dword destination",
            instruction: &[0x0F, 0xBF, 0x2B],
            apx: false,
            source_address: DATA,
            source: 0x8001,
            fs_base: 0,
            rax: 0xA5A5_5A5A_1357_2468,
            rcx: 0x8877_6655_4433_2211,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x1234_5678_9ABC_DEF0,
            destination_index: 5,
            expected_destination: 0x0000_0000_FFFF_8001,
        },
        Case {
            name: "REX2 MOVZX R16,byte [RBX] state-backed EGPR",
            instruction: &[0xD5, 0xC8, 0xB6, 0x03],
            apx: true,
            source_address: DATA,
            source: 0xA7,
            fs_base: 0,
            rax: 0xA5A5_5A5A_1357_2468,
            rcx: 0x8877_6655_4433_2211,
            rbx: DATA,
            rsp: 0x18_0000,
            rbp: 0x19_0000,
            destination_index: 16,
            expected_destination: 0xA7,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, memory: &Arc<GuestMemoryMmap>| {
            memory
                .write_obj(case.source, GuestAddress(case.source_address))
                .unwrap();
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = case.rax;
            regs.rcx = case.rcx;
            regs.rdx = 0x1122_3344_5566_7788;
            regs.rbx = case.rbx;
            regs.rsp = case.rsp;
            regs.rbp = case.rbp;
            regs.rsi = 0x99AA_BBCC_DDEE_FF00;
            regs.rdi = 0x0F1E_2D3C_4B5A_6978;
            regs.r8 = 0x0102_0304_0506_0708;
            regs.r9 = 0x1112_1314_1516_1718;
            regs.r10 = 0x2122_2324_2526_2728;
            regs.r16 = 0xA1A2_A3A4_A5A6_A7A8;
            regs.r17 = 0xB1B2_B3B4_B5B6_B7B8;
            regs.r31 = 0xF1F2_F3F4_F5F6_F7F8;
            regs.rflags = INITIAL_RFLAGS;
            vcpu.set_regs(&regs).unwrap();
            if case.fs_base != 0 {
                let mut sregs = vcpu.get_sregs().unwrap();
                sregs.fs.base = case.fs_base;
                vcpu.set_sregs(&sregs).unwrap();
            }
            vcpu.set_apx_enabled(case.apx);
        };

        let (mut interp, interp_mem) = make_vcpu_mem(&code);
        setup(&mut interp, &interp_mem);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        assert_eq!(
            gprs(&expected)[case.destination_index],
            case.expected_destination,
            "{} reference destination",
            case.name
        );

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the helper-backed native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
        assert_eq!(
            jit_mem
                .read_obj::<u64>(GuestAddress(case.source_address))
                .unwrap(),
            case.source,
            "{} source memory",
            case.name
        );
    }

    // A prior architectural write commits; the current extension and a later
    // write do not commit when the helper cannot read the source page.
    let fault_code = [
        0xB8, 0x78, 0x56, 0x34, 0x12, // mov eax,0x12345678
        0x48, 0x0F, 0xBE, 0x0B, // movsx rcx,byte [rbx]
        0x41, 0xB8, 0x01, 0x00, 0x00, 0x00, // mov r8d,1 (must not execute)
        0xF4,
    ];
    let (mut fault, _) = make_vcpu_mem(&fault_code);
    let mut before = fault.get_regs().unwrap();
    before.rax = u64::MAX;
    before.rcx = 0x8877_6655_4433_2211;
    before.rbx = MEM_SIZE + 0x1000;
    before.r8 = 0x0F0E_0D0C_0B0A_0908;
    before.rflags = INITIAL_RFLAGS;
    fault.set_regs(&before).unwrap();
    assert!(
        fault
            .jit_try_block()
            .expect("faulting helper-backed MOVSX JIT"),
        "memory MOVSX must compile before its precise helper fault"
    );
    let after = fault.get_regs().unwrap();
    assert_eq!(after.rax, 0x1234_5678, "prior EAX write must commit");
    assert_eq!(after.rcx, before.rcx, "faulting MOVSX must not commit");
    assert_eq!(after.rbx, before.rbx, "address base must remain unchanged");
    assert_eq!(after.r8, before.r8, "post-fault write must not execute");
    assert_eq!(after.rflags, before.rflags, "MOVSX fault preserves RFLAGS");
    assert_eq!(after.rip, LOAD_ADDR + 5, "fault restarts at MOVSX");
}

/// State-backed register NEG retains exact arithmetic flags for legacy/APX NDD
/// forms and preserves every incoming flag for APX NF forms.
#[test]
fn jit_state_backed_gpr_neg_executes_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        destination_index: usize,
        source_index: usize,
        expected_destination: u64,
        expected_source: u64,
    }

    let cases = [
        Case {
            name: "NEG BPL partial flag-setting in-place destination",
            instruction: &[0x40, 0xF6, 0xDD],
            apx: false,
            destination_index: 5,
            source_index: 5,
            expected_destination: 0x3344_5566_8765_9A44,
            expected_source: 0x3344_5566_8765_9A44,
        },
        Case {
            name: "NEG RSP full flag-setting in-place destination",
            instruction: &[0x48, 0xF7, 0xDC],
            apx: false,
            destination_index: 4,
            source_index: 4,
            expected_destination: 0xDDCC_BBAA_9988_A988,
            expected_source: 0xDDCC_BBAA_9988_A988,
        },
        Case {
            name: "APX NEG BPL,R16B partial destination",
            instruction: &[0x62, 0xFC, 0x54, 0x18, 0xF6, 0xD8],
            apx: true,
            destination_index: 5,
            source_index: 16,
            expected_destination: 0x3344_5566_8765_9A78,
            expected_source: 0xAABB_CCDD_EEFF_7788,
        },
        Case {
            name: "APX NF NEG R16W,SP partial destination",
            instruction: &[0x62, 0xF4, 0x7D, 0x14, 0xF7, 0xDC],
            apx: true,
            destination_index: 16,
            source_index: 4,
            expected_destination: 0xAABB_CCDD_EEFF_A988,
            expected_source: 0x2233_4455_6677_5678,
        },
        Case {
            name: "APX NEG R31D,EBP zero-extending destination",
            instruction: &[0x62, 0xF4, 0x04, 0x10, 0xF7, 0xDD],
            apx: true,
            destination_index: 31,
            source_index: 5,
            expected_destination: 0x0000_0000_789A_6544,
            expected_source: 0x3344_5566_8765_9ABC,
        },
        Case {
            name: "APX NF NEG R31,RSP full state-to-state destination",
            instruction: &[0x62, 0xF4, 0x84, 0x14, 0xF7, 0xDC],
            apx: true,
            destination_index: 31,
            source_index: 4,
            expected_destination: 0xDDCC_BBAA_9988_A988,
            expected_source: 0x2233_4455_6677_5678,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let setup = |vcpu: &mut X86_64Vcpu, apx: bool| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = 0x1122_3344_5566_1234;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5678;
        regs.rbp = 0x3344_5566_8765_9ABC;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_EEFF_7788;
        regs.r17 = 0xBBCC_DDEE_5566_7788;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
        vcpu.set_apx_enabled(apx);
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, case.apx);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        assert_eq!(
            gprs(&expected)[case.destination_index],
            case.expected_destination,
            "{} reference destination",
            case.name
        );
        assert_eq!(
            gprs(&expected)[case.source_index],
            case.expected_source,
            "{} reference source",
            case.name
        );

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, case.apx);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// State-backed register INC/DEC retain CF while updating OF/SF/ZF/AF/PF for
/// legacy/APX NDD forms and preserve every incoming flag for APX NF forms.
#[test]
fn jit_state_backed_gpr_inc_dec_execute_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        destination_index: usize,
        source_index: usize,
        expected_destination: u64,
        expected_source: u64,
    }

    let cases = [
        Case {
            name: "INC BPL partial flag-setting in-place destination",
            instruction: &[0x40, 0xFE, 0xC5],
            apx: false,
            destination_index: 5,
            source_index: 5,
            expected_destination: 0x3344_5566_8765_9ABE,
            expected_source: 0x3344_5566_8765_9ABE,
        },
        Case {
            name: "DEC RSP full flag-setting in-place destination",
            instruction: &[0x48, 0xFF, 0xCC],
            apx: false,
            destination_index: 4,
            source_index: 4,
            expected_destination: 0x2233_4455_6677_5677,
            expected_source: 0x2233_4455_6677_5677,
        },
        Case {
            name: "APX INC BPL,R16B partial destination",
            instruction: &[0x62, 0xFC, 0x54, 0x18, 0xFE, 0xC0],
            apx: true,
            destination_index: 5,
            source_index: 16,
            expected_destination: 0x3344_5566_8765_9A8B,
            expected_source: 0xAABB_CCDD_EEFF_778A,
        },
        Case {
            name: "APX NF DEC R16W,SP partial destination",
            instruction: &[0x62, 0xF4, 0x7D, 0x14, 0xFF, 0xCC],
            apx: true,
            destination_index: 16,
            source_index: 4,
            expected_destination: 0xAABB_CCDD_EEFF_5677,
            expected_source: 0x2233_4455_6677_5678,
        },
        Case {
            name: "APX INC R31D,EBP zero-extending destination",
            instruction: &[0x62, 0xF4, 0x04, 0x10, 0xFF, 0xC5],
            apx: true,
            destination_index: 31,
            source_index: 5,
            expected_destination: 0x0000_0000_8765_9ABE,
            expected_source: 0x3344_5566_8765_9ABD,
        },
        Case {
            name: "APX NF INC R31,RSP full state-to-state destination",
            instruction: &[0x62, 0xF4, 0x84, 0x14, 0xFF, 0xC4],
            apx: true,
            destination_index: 31,
            source_index: 4,
            expected_destination: 0x2233_4455_6677_5679,
            expected_source: 0x2233_4455_6677_5678,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let setup = |vcpu: &mut X86_64Vcpu, apx: bool| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = 0x1122_3344_5566_1234;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5678;
        regs.rbp = 0x3344_5566_8765_9ABD;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_EEFF_778A;
        regs.r17 = 0xBBCC_DDEE_5566_7788;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
        vcpu.set_apx_enabled(apx);
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, case.apx);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        assert_eq!(
            gprs(&expected)[case.destination_index],
            case.expected_destination,
            "{} reference destination",
            case.name
        );
        assert_eq!(
            gprs(&expected)[case.source_index],
            case.expected_source,
            "{} reference source",
            case.name
        );

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, case.apx);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// State-backed scalar counts retain exact legacy POPCNT/LZCNT/TZCNT flags and
/// preserve every incoming flag for APX NF forms.
#[test]
fn jit_state_backed_gpr_count_executes_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        destination_index: usize,
        source_index: usize,
        source: u64,
        expected_destination: u64,
    }

    let cases = [
        Case {
            name: "POPCNT BP,SP partial flag-setting destination",
            instruction: &[0x66, 0xF3, 0x0F, 0xB8, 0xEC],
            apx: false,
            destination_index: 5,
            source_index: 4,
            source: 0x2233_4455_6677_5678,
            expected_destination: 0x3344_5566_8765_0008,
        },
        Case {
            name: "TZCNT RSP,RBP full flag-merge destination",
            instruction: &[0xF3, 0x48, 0x0F, 0xBC, 0xE5],
            apx: false,
            destination_index: 4,
            source_index: 5,
            source: 0,
            expected_destination: 64,
        },
        Case {
            name: "LZCNT R8D,EBP state-backed source",
            instruction: &[0xF3, 0x44, 0x0F, 0xBD, 0xC5],
            apx: false,
            destination_index: 8,
            source_index: 5,
            source: 0x3344_5566_8000_0000,
            expected_destination: 0,
        },
        Case {
            name: "APX NF POPCNT R16W,SP partial destination",
            instruction: &[0x62, 0xE4, 0x7D, 0x0C, 0x88, 0xC4],
            apx: true,
            destination_index: 16,
            source_index: 4,
            source: 0x2233_4455_6677_5678,
            expected_destination: 0xAABB_CCDD_EEFF_0008,
        },
        Case {
            name: "APX NF LZCNT R31D,EBP zero-extending destination",
            instruction: &[0x62, 0x64, 0x7C, 0x0C, 0xF5, 0xFD],
            apx: true,
            destination_index: 31,
            source_index: 5,
            source: 1,
            expected_destination: 31,
        },
        Case {
            name: "APX NF TZCNT R31,RSP full destination",
            instruction: &[0x62, 0x64, 0xFC, 0x0C, 0xF4, 0xFC],
            apx: true,
            destination_index: 31,
            source_index: 4,
            source: 0x80,
            expected_destination: 7,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let setup = |vcpu: &mut X86_64Vcpu, case: &Case| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = 0x1122_3344_5566_1234;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5678;
        regs.rbp = 0x3344_5566_8765_9ABC;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_EEFF_7788;
        regs.r17 = 0xBBCC_DDEE_5566_7788;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        let source = match case.source_index {
            4 => &mut regs.rsp,
            5 => &mut regs.rbp,
            _ => unreachable!(),
        };
        *source = case.source;
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
        vcpu.set_apx_enabled(case.apx);
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, &case);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        assert_eq!(
            gprs(&expected)[case.destination_index],
            case.expected_destination,
            "{} reference destination",
            case.name
        );
        assert_eq!(
            gprs(&expected)[case.source_index],
            case.source,
            "{} reference source",
            case.name
        );

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, &case);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// State-backed BSF/BSR execute through a GuestRegs snapshot so guest RSP/RBP
/// and APX EGPRs never alias the native stack. Only ZF changes; zero sources
/// retain the pre-instruction destination in parity with the interpreter.
#[test]
fn jit_state_backed_gpr_bit_scan_executes_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        destination_index: usize,
        source_index: usize,
        source: u64,
        expected_destination: u64,
    }

    let cases = [
        Case {
            name: "BSF BP,SP partial destination",
            instruction: &[0x66, 0x0F, 0xBC, 0xEC],
            apx: false,
            destination_index: 5,
            source_index: 4,
            source: 0x2233_4455_6677_8000,
            expected_destination: 0x3344_5566_8765_000F,
        },
        Case {
            name: "zero BSR RSP,RBP full destination",
            instruction: &[0x48, 0x0F, 0xBD, 0xE5],
            apx: false,
            destination_index: 4,
            source_index: 5,
            source: 0,
            expected_destination: 0x2233_4455_6677_5678,
        },
        Case {
            name: "BSF RBP,RSP full destination",
            instruction: &[0x48, 0x0F, 0xBC, 0xEC],
            apx: false,
            destination_index: 5,
            source_index: 4,
            source: 0x100,
            expected_destination: 8,
        },
        Case {
            name: "REX2 BSF R31,R16 extended destination",
            instruction: &[0xD5, 0xDC, 0xBC, 0xF8],
            apx: true,
            destination_index: 31,
            source_index: 16,
            source: 0x100,
            expected_destination: 8,
        },
        Case {
            name: "REX2 BSR R16D,R31D zero-extending destination",
            instruction: &[0xD5, 0xD1, 0xBD, 0xC7],
            apx: true,
            destination_index: 16,
            source_index: 31,
            source: 0x8000_0000,
            expected_destination: 31,
        },
        Case {
            name: "zero REX2 BSF R16W,SP partial destination",
            instruction: &[0x66, 0xD5, 0xC0, 0xBC, 0xC4],
            apx: true,
            destination_index: 16,
            source_index: 4,
            source: 0,
            expected_destination: 0xAABB_CCDD_EEFF_7788,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let setup = |vcpu: &mut X86_64Vcpu, case: &Case| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = 0x1122_3344_5566_1234;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5678;
        regs.rbp = 0x3344_5566_8765_9ABC;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_EEFF_7788;
        regs.r17 = 0xBBCC_DDEE_5566_7788;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        match case.source_index {
            4 => regs.rsp = case.source,
            5 => regs.rbp = case.source,
            16 => regs.r16 = case.source,
            31 => regs.r31 = case.source,
            _ => unreachable!(),
        }
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
        vcpu.set_apx_enabled(case.apx);
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, &case);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        assert_eq!(
            gprs(&expected)[case.destination_index],
            case.expected_destination,
            "{} reference destination",
            case.name
        );
        if case.destination_index != case.source_index {
            assert_eq!(
                gprs(&expected)[case.source_index],
                case.source,
                "{} reference source",
                case.name
            );
        }

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, &case);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// Register BT/BTS/BTR/BTC use state-backed operand and index staging when
/// guest RSP/RBP or APX EGPRs participate. CF is merged exactly while every
/// undefined status flag and every non-destination GPR remains unchanged.
#[test]
fn jit_state_backed_gpr_bit_test_executes_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        operand_index: usize,
        index_index: Option<usize>,
        source: u64,
        bit_index: u64,
        expected_operand: u64,
    }

    let cases = [
        Case {
            name: "BT RSP,RBP register index",
            instruction: &[0x48, 0x0F, 0xA3, 0xEC],
            apx: false,
            operand_index: 4,
            index_index: Some(5),
            source: 1u64 << 63,
            bit_index: 63,
            expected_operand: 1u64 << 63,
        },
        Case {
            name: "BTS BP,15 partial destination",
            instruction: &[0x66, 0x0F, 0xBA, 0xED, 0x0F],
            apx: false,
            operand_index: 5,
            index_index: None,
            source: 0x3344_5566_8765_0000,
            bit_index: 15,
            expected_operand: 0x3344_5566_8765_8000,
        },
        Case {
            name: "REX2 BTR R16D,R31D zero-extending destination",
            instruction: &[0xD5, 0xD4, 0xB3, 0xF8],
            apx: true,
            operand_index: 16,
            index_index: Some(31),
            source: u64::MAX,
            bit_index: 31,
            expected_operand: 0x7FFF_FFFF,
        },
        Case {
            name: "REX2 BTC R31,R16 extended destination and index",
            instruction: &[0xD5, 0xD9, 0xBB, 0xC7],
            apx: true,
            operand_index: 31,
            index_index: Some(16),
            source: 0,
            bit_index: 63,
            expected_operand: 1u64 << 63,
        },
        Case {
            name: "BTR RSP,63 full destination",
            instruction: &[0x48, 0x0F, 0xBA, 0xF4, 0x3F],
            apx: false,
            operand_index: 4,
            index_index: None,
            source: 1u64 << 63,
            bit_index: 63,
            expected_operand: 0,
        },
        Case {
            name: "REX2 BT R16W,SP masked register index",
            instruction: &[0x66, 0xD5, 0x90, 0xA3, 0xE0],
            apx: true,
            operand_index: 16,
            index_index: Some(4),
            source: 1,
            bit_index: 16,
            expected_operand: 1,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let setup = |vcpu: &mut X86_64Vcpu, case: &Case| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = 0x1122_3344_5566_1234;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5678;
        regs.rbp = 0x3344_5566_8765_9ABC;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_EEFF_7788;
        regs.r17 = 0xBBCC_DDEE_5566_7788;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        match case.operand_index {
            4 => regs.rsp = case.source,
            5 => regs.rbp = case.source,
            16 => regs.r16 = case.source,
            31 => regs.r31 = case.source,
            _ => unreachable!(),
        }
        if let Some(index) = case.index_index {
            match index {
                4 => regs.rsp = case.bit_index,
                5 => regs.rbp = case.bit_index,
                16 => regs.r16 = case.bit_index,
                31 => regs.r31 = case.bit_index,
                _ => unreachable!(),
            }
        }
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
        vcpu.set_apx_enabled(case.apx);
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, &case);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        assert_eq!(
            gprs(&expected)[case.operand_index],
            case.expected_operand,
            "{} reference operand",
            case.name
        );

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, &case);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// Register CRC32C stages guest RSP/RBP through GuestRegs while retaining the
/// instruction's full-GPR zero-extension and no-flags-modified contracts.
#[test]
fn jit_state_backed_gpr_crc32c_executes_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        dst_index: usize,
        data_index: usize,
        accumulator: u64,
        source: u64,
    }
    let cases = [
        Case {
            name: "CRC32 EBP,BPL alias",
            instruction: &[0xF2, 0x40, 0x0F, 0x38, 0xF0, 0xED],
            dst_index: 5,
            data_index: 5,
            accumulator: 0x1234_56A5,
            source: 0x1234_56A5,
        },
        Case {
            name: "CRC32 ESP,BP",
            instruction: &[0x66, 0xF2, 0x0F, 0x38, 0xF1, 0xE5],
            dst_index: 4,
            data_index: 5,
            accumulator: 0x89AB_CDEF,
            source: 0x0123_4567_89AB_BEEF,
        },
        Case {
            name: "CRC32 EBP,ESP",
            instruction: &[0xF2, 0x0F, 0x38, 0xF1, 0xEC],
            dst_index: 5,
            data_index: 4,
            accumulator: 0x1020_3040,
            source: 0xAABB_CCDD_DEAD_BEEF,
        },
        Case {
            name: "CRC32 RSP,RBP",
            instruction: &[0xF2, 0x48, 0x0F, 0x38, 0xF1, 0xE5],
            dst_index: 4,
            data_index: 5,
            accumulator: 0x7654_3210,
            source: 0x0123_4567_89AB_CDEF,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let setup = |vcpu: &mut X86_64Vcpu, case: &Case| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = 0x1122_3344_5566_1234;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5678;
        regs.rbp = 0x3344_5566_8765_9ABC;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_EEFF_7788;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        match case.dst_index {
            4 => regs.rsp = case.accumulator,
            5 => regs.rbp = case.accumulator,
            _ => unreachable!(),
        }
        if case.data_index != case.dst_index {
            match case.data_index {
                4 => regs.rsp = case.source,
                5 => regs.rbp = case.source,
                _ => unreachable!(),
            }
        }
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, &case);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        assert_eq!(
            gprs(&expected)[case.dst_index] >> 32,
            0,
            "{} reference result must zero-extend",
            case.name
        );

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, &case);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// Register PDEP/PEXT stage guest RSP/RBP operands through GuestRegs while
/// retaining native aliasing, dword zero-extension, and unchanged RFLAGS.
#[test]
fn jit_state_backed_gpr_pdep_pext_execute_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        dst_index: usize,
        src_index: usize,
        mask_index: usize,
        source: u64,
        mask: u64,
        dword: bool,
    }
    let cases = [
        Case {
            name: "PDEP RSP,RBP,R8",
            instruction: &[0xC4, 0xC2, 0xD3, 0xF5, 0xE0],
            dst_index: 4,
            src_index: 5,
            mask_index: 8,
            source: 0x0123_4567_89AB_CDEF,
            mask: 0xF0F0_00FF_AA55_5A5A,
            dword: false,
        },
        Case {
            name: "PEXT RBP,RSP,RCX",
            instruction: &[0xC4, 0xE2, 0xDA, 0xF5, 0xE9],
            dst_index: 5,
            src_index: 4,
            mask_index: 1,
            source: 0xAABB_CCDD_DEAD_BEEF,
            mask: 0x0F0F_F0F0_55AA_AA55,
            dword: false,
        },
        Case {
            name: "PDEP R8D,ESP,ESP source-mask alias",
            instruction: &[0xC4, 0x62, 0x5B, 0xF5, 0xC4],
            dst_index: 8,
            src_index: 4,
            mask_index: 4,
            source: 0xDEAD_BEEF_1357_2468,
            mask: 0xA5A5_5A5A_C3C3_3C3C,
            dword: true,
        },
        Case {
            name: "PEXT EBP,EBP,EBP all operands alias",
            instruction: &[0xC4, 0xE2, 0x52, 0xF5, 0xED],
            dst_index: 5,
            src_index: 5,
            mask_index: 5,
            source: 0xFEDC_BA98_7654_3210,
            mask: 0xAAAA_5555_F0F0_0F0F,
            dword: true,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let set_gpr = |regs: &mut Registers, index: usize, value: u64| match index {
        1 => regs.rcx = value,
        4 => regs.rsp = value,
        5 => regs.rbp = value,
        8 => regs.r8 = value,
        _ => unreachable!(),
    };
    let setup = |vcpu: &mut X86_64Vcpu, case: &Case| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = 0x1122_3344_5566_1234;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5678;
        regs.rbp = 0x3344_5566_8765_9ABC;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_EEFF_7788;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        set_gpr(&mut regs, case.src_index, case.source);
        set_gpr(&mut regs, case.mask_index, case.mask);
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, &case);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        if case.dword {
            assert_eq!(
                gprs(&expected)[case.dst_index] >> 32,
                0,
                "{} reference result must zero-extend",
                case.name
            );
        }

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, &case);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// Register BEXTR/BZHI stage guest RSP/RBP operands through GuestRegs while
/// retaining native aliasing, dword zero-extension, and deterministic merging
/// of defined and undefined status flags.
#[test]
fn jit_state_backed_gpr_bextr_bzhi_execute_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        dst_index: usize,
        src_index: usize,
        control_index: usize,
        source: u64,
        control: u64,
        dword: bool,
    }
    let cases = [
        Case {
            name: "BEXTR RSP,RBP,R8",
            instruction: &[0xC4, 0xE2, 0xB8, 0xF7, 0xE5],
            dst_index: 4,
            src_index: 5,
            control_index: 8,
            source: 0xFEDC_BA98_7654_3210,
            control: (20 << 8) | 12,
            dword: false,
        },
        Case {
            name: "BZHI RBP,RSP,RCX",
            instruction: &[0xC4, 0xE2, 0xF0, 0xF5, 0xEC],
            dst_index: 5,
            src_index: 4,
            control_index: 1,
            source: 0x8000_0000_1234_5678,
            control: 64,
            dword: false,
        },
        Case {
            name: "BEXTR R8D,ESP,ESP source-control alias",
            instruction: &[0xC4, 0x62, 0x58, 0xF7, 0xC4],
            dst_index: 8,
            src_index: 4,
            control_index: 4,
            source: 0,
            control: (12 << 8) | 7,
            dword: true,
        },
        Case {
            name: "BZHI EBP,EBP,EBP all operands alias",
            instruction: &[0xC4, 0xE2, 0x50, 0xF5, 0xED],
            dst_index: 5,
            src_index: 5,
            control_index: 5,
            source: 0,
            control: 0x8000_0020,
            dword: true,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let set_gpr = |regs: &mut Registers, index: usize, value: u64| match index {
        1 => regs.rcx = value,
        4 => regs.rsp = value,
        5 => regs.rbp = value,
        8 => regs.r8 = value,
        _ => unreachable!(),
    };
    let setup = |vcpu: &mut X86_64Vcpu, case: &Case| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = 0x1122_3344_5566_1234;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5678;
        regs.rbp = 0x3344_5566_8765_9ABC;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_EEFF_7788;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        set_gpr(&mut regs, case.src_index, case.source);
        set_gpr(&mut regs, case.control_index, case.control);
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, &case);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        if case.dword {
            assert_eq!(
                gprs(&expected)[case.dst_index] >> 32,
                0,
                "{} reference result must zero-extend",
                case.name
            );
        }

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, &case);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// Register ANDN stages guest RSP/RBP operands through GuestRegs while
/// retaining all destination/source alias relationships, dword zero-extension,
/// and deterministic merging of CF/ZF/SF/OF with preserved PF/AF.
#[test]
fn jit_state_backed_gpr_andn_execute_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        dst_index: usize,
        src1_index: usize,
        src2_index: usize,
        src1_value: u64,
        src2_value: u64,
        dword: bool,
    }
    let cases = [
        Case {
            name: "ANDN RSP,RBP,R8",
            instruction: &[0xC4, 0xC2, 0xD0, 0xF2, 0xE0],
            dst_index: 4,
            src1_index: 8,
            src2_index: 5,
            src1_value: 0xF0F0_00FF_AA55_5A5A,
            src2_value: 0x70F0_F000_AA00_0A0A,
            dword: false,
        },
        Case {
            name: "ANDN RBP,RSP,RCX",
            instruction: &[0xC4, 0xE2, 0xD8, 0xF2, 0xE9],
            dst_index: 5,
            src1_index: 1,
            src2_index: 4,
            src1_value: 0x8000_0000_FFFF_00FF,
            src2_value: 0x7FFF_FFFF_FF00_0000,
            dword: false,
        },
        Case {
            name: "ANDN R8D,ESP,ESP source alias",
            instruction: &[0xC4, 0x62, 0x58, 0xF2, 0xC4],
            dst_index: 8,
            src1_index: 4,
            src2_index: 4,
            src1_value: 0xAABB_CCDD_8000_0018,
            src2_value: 0xAABB_CCDD_8000_0018,
            dword: true,
        },
        Case {
            name: "ANDN EBP,EBP,EBP all operands alias",
            instruction: &[0xC4, 0xE2, 0x50, 0xF2, 0xED],
            dst_index: 5,
            src1_index: 5,
            src2_index: 5,
            src1_value: 0xDEAD_BEEF_1357_2418,
            src2_value: 0xDEAD_BEEF_1357_2418,
            dword: true,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let set_gpr = |regs: &mut Registers, index: usize, value: u64| match index {
        1 => regs.rcx = value,
        4 => regs.rsp = value,
        5 => regs.rbp = value,
        8 => regs.r8 = value,
        _ => unreachable!(),
    };
    let setup = |vcpu: &mut X86_64Vcpu, case: &Case| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = 0x1122_3344_5566_1234;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5678;
        regs.rbp = 0x3344_5566_8765_9ABC;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_EEFF_7788;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        set_gpr(&mut regs, case.src1_index, case.src1_value);
        set_gpr(&mut regs, case.src2_index, case.src2_value);
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, &case);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        if case.dword {
            assert_eq!(
                gprs(&expected)[case.dst_index] >> 32,
                0,
                "{} reference result must zero-extend",
                case.name
            );
        }

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, &case);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// Register BLSR/BLSMSK/BLSI stage guest RSP/RBP operands through GuestRegs
/// while retaining native aliasing, dword zero-extension, and deterministic
/// merging of the defined CF/ZF/SF/OF subset with preserved PF/AF.
#[test]
fn jit_state_backed_gpr_bls_execute_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        dst_index: usize,
        src_index: usize,
        source: u64,
        dword: bool,
    }
    let cases = [
        Case {
            name: "BLSR RSP,RBP",
            instruction: &[0xC4, 0xE2, 0xD8, 0xF3, 0xCD],
            dst_index: 4,
            src_index: 5,
            source: 0,
            dword: false,
        },
        Case {
            name: "BLSMSK RBP,RSP",
            instruction: &[0xC4, 0xE2, 0xD0, 0xF3, 0xD4],
            dst_index: 5,
            src_index: 4,
            source: 0,
            dword: false,
        },
        Case {
            name: "BLSI R8D,ESP",
            instruction: &[0xC4, 0xE2, 0x38, 0xF3, 0xDC],
            dst_index: 8,
            src_index: 4,
            source: 0xAABB_CCDD_8000_0018,
            dword: true,
        },
        Case {
            name: "BLSR EBP,EBP source-destination alias",
            instruction: &[0xC4, 0xE2, 0x50, 0xF3, 0xCD],
            dst_index: 5,
            src_index: 5,
            source: 0xDEAD_BEEF_1357_2418,
            dword: true,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let set_gpr = |regs: &mut Registers, index: usize, value: u64| match index {
        4 => regs.rsp = value,
        5 => regs.rbp = value,
        _ => unreachable!(),
    };
    let setup = |vcpu: &mut X86_64Vcpu, case: &Case| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = 0x1122_3344_5566_1234;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5678;
        regs.rbp = 0x3344_5566_8765_9ABC;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_EEFF_7788;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        set_gpr(&mut regs, case.src_index, case.source);
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, &case);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        if case.dword {
            assert_eq!(
                gprs(&expected)[case.dst_index] >> 32,
                0,
                "{} reference result must zero-extend",
                case.name
            );
        }

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, &case);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// State-backed register NOT uses the GuestRegs snapshot for guest RSP/RBP and
/// APX EGPRs while retaining byte/word partial writes and dword zero extension.
#[test]
fn jit_state_backed_gpr_not_executes_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        destination_index: usize,
        source_index: usize,
        expected_destination: u64,
        expected_source: u64,
    }

    let cases = [
        Case {
            name: "NOT BPL partial in-place destination",
            instruction: &[0x40, 0xF6, 0xD5],
            apx: false,
            destination_index: 5,
            source_index: 5,
            expected_destination: 0x3344_5566_8765_9A43,
            expected_source: 0x3344_5566_8765_9A43,
        },
        Case {
            name: "NOT RSP full in-place destination",
            instruction: &[0x48, 0xF7, 0xD4],
            apx: false,
            destination_index: 4,
            source_index: 4,
            expected_destination: 0xDDCC_BBAA_9988_A987,
            expected_source: 0xDDCC_BBAA_9988_A987,
        },
        Case {
            name: "APX NOT BPL,R16B partial destination",
            instruction: &[0x62, 0xFC, 0x54, 0x18, 0xF6, 0xD0],
            apx: true,
            destination_index: 5,
            source_index: 16,
            expected_destination: 0x3344_5566_8765_9A77,
            expected_source: 0xAABB_CCDD_EEFF_7788,
        },
        Case {
            name: "APX NOT R16W,SP partial destination",
            instruction: &[0x62, 0xF4, 0x7D, 0x10, 0xF7, 0xD4],
            apx: true,
            destination_index: 16,
            source_index: 4,
            expected_destination: 0xAABB_CCDD_EEFF_A987,
            expected_source: 0x2233_4455_6677_5678,
        },
        Case {
            name: "APX NOT R31D,EBP zero-extending destination",
            instruction: &[0x62, 0xF4, 0x04, 0x10, 0xF7, 0xD5],
            apx: true,
            destination_index: 31,
            source_index: 5,
            expected_destination: 0x0000_0000_789A_6543,
            expected_source: 0x3344_5566_8765_9ABC,
        },
        Case {
            name: "APX NOT R31,RSP full state-to-state destination",
            instruction: &[0x62, 0xF4, 0x84, 0x10, 0xF7, 0xD4],
            apx: true,
            destination_index: 31,
            source_index: 4,
            expected_destination: 0xDDCC_BBAA_9988_A987,
            expected_source: 0x2233_4455_6677_5678,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let setup = |vcpu: &mut X86_64Vcpu, apx: bool| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = 0x1122_3344_5566_1234;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5678;
        regs.rbp = 0x3344_5566_8765_9ABC;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_EEFF_7788;
        regs.r17 = 0xBBCC_DDEE_5566_7788;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
        vcpu.set_apx_enabled(apx);
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, case.apx);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        assert_eq!(
            gprs(&expected)[case.destination_index],
            case.expected_destination,
            "{} reference destination",
            case.name
        );
        assert_eq!(
            gprs(&expected)[case.source_index],
            case.expected_source,
            "{} reference source",
            case.name
        );

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, case.apx);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// Register byte swaps are flag-neutral and must remain native for both the
/// legacy in-place BSWAP encoding and APX MOVBE's two-register word form.
#[test]
fn jit_bswap_and_apx_movbe_preserve_partial_registers_and_flags() {
    const STATUS_MASK: u64 = 0x08D5;
    for (name, instruction, apx, rax, r8, expected_r8) in [
        (
            "legacy bswap r8",
            &[0x49, 0x0F, 0xC8][..],
            false,
            0,
            0x0123_4567_89AB_CDEF,
            0xEFCD_AB89_6745_2301,
        ),
        (
            "APX movbe r8w,ax",
            &[0x62, 0xD4, 0x7D, 0x08, 0x61, 0xC0][..],
            true,
            0x1122_3344_5566_1234,
            0xAABB_CCDD_EEFF_7788,
            0xAABB_CCDD_EEFF_3412,
        ),
    ] {
        // loop: dec ecx; jnz loop
        //       xor r9d,r9d       ; known status = ZF|PF
        //       <byte swap>
        //       hlt
        let mut code = vec![0xFF, 0xC9, 0x75, 0xFC, 0x45, 0x31, 0xC9];
        code.extend_from_slice(instruction);
        code.push(0xF4);

        let setup = |vcpu: &mut X86_64Vcpu| {
            vcpu.set_apx_enabled(apx);
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = rax;
            regs.rcx = 200;
            regs.r8 = r8;
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp);
        run_interp(&mut interp);

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("JIT {name}: {error:?}")),
            "{name} loop must enter the native tier"
        );
        run_interp(&mut jit);

        let expected = interp.get_regs().unwrap();
        let after = jit.get_regs().unwrap();
        assert_eq!(after.r8, expected.r8, "{name}: result vs interpreter");
        assert_eq!(after.r8, expected_r8, "{name}: architectural result");
        assert_eq!(after.rax, rax, "{name}: independent source preservation");
        assert_eq!(after.rcx & 0xFFFF_FFFF, 0, "{name}: loop count");
        assert_eq!(after.rflags & STATUS_MASK, 0x44, "{name}: status flags");
        assert_eq!(
            after.rflags & STATUS_MASK,
            expected.rflags & STATUS_MASK,
            "{name}: native flags vs interpreter"
        );
    }
}

/// State-backed register BSWAP and APX register MOVBE use the GuestRegs
/// snapshot rather than host RSP/RBP or nonexistent host EGPR mappings.
#[test]
fn jit_state_backed_gpr_bswap_executes_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        compare_interpreter: bool,
        destination_index: usize,
        source_index: usize,
        expected_destination: u64,
        expected_source: u64,
    }

    let cases = [
        Case {
            name: "BSWAP RSP full in-place destination",
            instruction: &[0x48, 0x0F, 0xCC],
            apx: false,
            compare_interpreter: true,
            destination_index: 4,
            source_index: 4,
            expected_destination: 0x7856_7766_5544_3322,
            expected_source: 0x7856_7766_5544_3322,
        },
        Case {
            name: "BSWAP RBP full in-place destination",
            instruction: &[0x48, 0x0F, 0xCD],
            apx: false,
            compare_interpreter: true,
            destination_index: 5,
            source_index: 5,
            expected_destination: 0xBC9A_6587_6655_4433,
            expected_source: 0xBC9A_6587_6655_4433,
        },
        Case {
            name: "REX2 BSWAP R16 full in-place destination",
            instruction: &[0xD5, 0x98, 0xC8],
            apx: true,
            compare_interpreter: false,
            destination_index: 16,
            source_index: 16,
            expected_destination: 0x8877_FFEE_DDCC_BBAA,
            expected_source: 0x8877_FFEE_DDCC_BBAA,
        },
        Case {
            name: "REX2 BSWAP R16D zero-extending in-place destination",
            instruction: &[0xD5, 0x90, 0xC8],
            apx: true,
            compare_interpreter: false,
            destination_index: 16,
            source_index: 16,
            expected_destination: 0x0000_0000_8877_FFEE,
            expected_source: 0x0000_0000_8877_FFEE,
        },
        Case {
            name: "APX MOVBE BP,R16W partial destination",
            instruction: &[0x62, 0xE4, 0x7D, 0x08, 0x61, 0xC5],
            apx: true,
            compare_interpreter: true,
            destination_index: 5,
            source_index: 16,
            expected_destination: 0x3344_5566_8765_8877,
            expected_source: 0xAABB_CCDD_EEFF_7788,
        },
        Case {
            name: "APX MOVBE R16D,R17D zero-extending destination",
            instruction: &[0x62, 0xEC, 0x7C, 0x08, 0x61, 0xC8],
            apx: true,
            compare_interpreter: true,
            destination_index: 16,
            source_index: 17,
            expected_destination: 0x0000_0000_8877_6655,
            expected_source: 0xBBCC_DDEE_5566_7788,
        },
        Case {
            name: "APX MOVBE R31,RSP full state-to-state copy",
            instruction: &[0x62, 0xDC, 0xFC, 0x08, 0x61, 0xE7],
            apx: true,
            compare_interpreter: true,
            destination_index: 31,
            source_index: 4,
            expected_destination: 0x7856_7766_5544_3322,
            expected_source: 0x2233_4455_6677_5678,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let setup = |vcpu: &mut X86_64Vcpu, apx: bool| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = 0x1122_3344_5566_1234;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5678;
        regs.rbp = 0x3344_5566_8765_9ABC;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_EEFF_7788;
        regs.r17 = 0xBBCC_DDEE_5566_7788;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        regs.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&regs).unwrap();
        vcpu.set_apx_enabled(apx);
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let interpreter_expected = case.compare_interpreter.then(|| {
            let mut interp = make_vcpu_code(&code);
            setup(&mut interp, case.apx);
            assert!(
                interp.step().unwrap().is_none(),
                "{} interpreter",
                case.name
            );
            interp.get_regs().unwrap()
        });

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, case.apx);
        let before = jit.get_regs().unwrap();
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(
            gprs(&actual)[case.destination_index],
            case.expected_destination,
            "{} architectural destination",
            case.name
        );
        assert_eq!(
            gprs(&actual)[case.source_index],
            case.expected_source,
            "{} architectural source",
            case.name
        );

        if let Some(expected) = interpreter_expected {
            assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
            assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
            assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
        } else {
            let mut expected_gprs = gprs(&before);
            expected_gprs[case.destination_index] = case.expected_destination;
            assert_eq!(gprs(&actual), expected_gprs, "{} GPR file", case.name);
            assert_eq!(actual.rflags, before.rflags, "{} RFLAGS", case.name);
            assert_eq!(
                actual.rip,
                LOAD_ADDR + case.instruction.len() as u64,
                "{} RIP",
                case.name
            );
        }
    }
}

/// CWD/CDQ/CQO have no explicit operands in machine code but lower from an
/// exact RAX-to-RDX IR shape. Exercise every architectural width, including
/// x86 partial-register writes and preservation of the preceding DEC flags.
#[test]
fn jit_cwd_cdq_cqo_preserve_partial_writes_source_and_flags() {
    const STATUS_MASK: u64 = 0x08D5;

    for (name, instruction, rax, rdx, expected_rdx) in [
        (
            "cwd",
            &[0x66, 0x99][..],
            0x1122_3344_5566_8001,
            0xAABB_CCDD_EEFF_1234,
            0xAABB_CCDD_EEFF_FFFF,
        ),
        (
            "cdq",
            &[0x99][..],
            0x1122_3344_8000_0001,
            u64::MAX,
            0x0000_0000_FFFF_FFFF,
        ),
        ("cqo", &[0x48, 0x99][..], 0x8000_0000_0000_0001, 0, u64::MAX),
    ] {
        // loop: dec r8d; jnz loop; CWD/CDQ/CQO; hlt
        let mut code = vec![0x41, 0xFF, 0xC8, 0x75, 0xFB];
        code.extend_from_slice(instruction);
        code.push(0xF4);

        let setup = |vcpu: &mut X86_64Vcpu| {
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = rax;
            regs.rdx = rdx;
            regs.r8 = 200;
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp);
        run_interp(&mut interp);

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("JIT {name}: {error:?}")),
            "{name} loop must enter the native tier"
        );
        run_interp(&mut jit);

        let expected = interp.get_regs().unwrap();
        let after = jit.get_regs().unwrap();
        assert_eq!(after.rax, expected.rax, "{name}: RAX vs interpreter");
        assert_eq!(after.rdx, expected.rdx, "{name}: RDX vs interpreter");
        assert_eq!(after.rax, rax, "{name}: RAX is unchanged");
        assert_eq!(after.rdx, expected_rdx, "{name}: architectural RDX");
        assert_eq!(after.r8 & 0xFFFF_FFFF, 0, "{name}: loop count");
        assert_eq!(
            after.rflags & STATUS_MASK,
            expected.rflags & STATUS_MASK,
            "{name}: status flags vs interpreter"
        );
        assert_eq!(after.rflags & STATUS_MASK, 0x45, "{name}: status flags");
    }
}

/// Immediate-one RCL/RCR define both CF and OF. Other status flags must remain
/// those produced by the loop's terminal DEC. This includes APX NDD lowering,
/// where the destination is distinct from the source but native code uses the
/// legacy two-operand rotate after an exact-width copy.
#[test]
fn jit_carry_rotates_immediate_one_preserve_width_alias_and_flags() {
    const STATUS_MASK: u64 = 0x08D5;

    for (name, instruction, apx, rax, r8, expected_rax, expected_r8, expected_status) in [
        (
            "rcl al,1",
            &[0xD0, 0xD0][..],
            false,
            0x1122_3344_5566_0042,
            0xAABB_CCDD_EEFF_0011,
            0x1122_3344_5566_0085,
            0xAABB_CCDD_EEFF_0011,
            0x844,
        ),
        (
            "rcr ax,1",
            &[0x66, 0xD1, 0xD8][..],
            false,
            0x1122_3344_5566_0001,
            0xAABB_CCDD_EEFF_0011,
            0x1122_3344_5566_8000,
            0xAABB_CCDD_EEFF_0011,
            0x845,
        ),
        (
            "rcl eax,1",
            &[0xD1, 0xD0][..],
            false,
            0xAABB_CCDD_4000_0000,
            0xAABB_CCDD_EEFF_0011,
            0x0000_0000_8000_0001,
            0xAABB_CCDD_EEFF_0011,
            0x844,
        ),
        (
            "rcr rax,1",
            &[0x48, 0xD1, 0xD8][..],
            false,
            1,
            0xAABB_CCDD_EEFF_0011,
            0x8000_0000_0000_0000,
            0xAABB_CCDD_EEFF_0011,
            0x845,
        ),
        (
            "APX NDD rcl rax,r8,1",
            &[0x62, 0xF4, 0xBC, 0x18, 0xD1, 0xD0][..],
            true,
            0x4000_0000_0000_0000,
            0xAABB_CCDD_EEFF_0011,
            0x4000_0000_0000_0000,
            0x8000_0000_0000_0001,
            0x844,
        ),
    ] {
        // loop: dec r9d; jnz loop; RCL/RCR; hlt
        let mut code = vec![0x41, 0xFF, 0xC9, 0x75, 0xFB];
        code.extend_from_slice(instruction);
        code.push(0xF4);

        let setup = |vcpu: &mut X86_64Vcpu| {
            vcpu.set_apx_enabled(apx);
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = rax;
            regs.r8 = r8;
            regs.r9 = 200;
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp);
        run_interp(&mut interp);

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("JIT {name}: {error:?}")),
            "{name} loop must enter the native tier"
        );
        run_interp(&mut jit);

        let expected = interp.get_regs().unwrap();
        let after = jit.get_regs().unwrap();
        assert_eq!(after.rax, expected.rax, "{name}: RAX vs interpreter");
        assert_eq!(after.r8, expected.r8, "{name}: R8 vs interpreter");
        assert_eq!(after.rax, expected_rax, "{name}: architectural RAX");
        assert_eq!(after.r8, expected_r8, "{name}: architectural R8");
        assert_eq!(after.r9 & 0xFFFF_FFFF, 0, "{name}: loop count");
        assert_eq!(
            after.rflags & STATUS_MASK,
            expected.rflags & STATUS_MASK,
            "{name}: status flags vs interpreter"
        );
        assert_eq!(
            after.rflags & STATUS_MASK,
            expected_status,
            "{name}: architectural status flags"
        );
    }
}

/// VEX ANDN defines CF/ZF/SF/OF while retaining the emulator's deterministic
/// values for undefined PF/AF. APX NF ANDN preserves every flag. Native
/// lowering must also retain all three destination/source alias relationships.
#[test]
fn jit_andn_vex_and_apx_nf_preserve_aliases_and_exact_flags() {
    const STATUS_MASK: u64 = 0x08D5;

    for (name, instruction, apx, direct_reference, initial, expected) in [
        (
            "VEX andn rax,rcx,rdx",
            &[0xC4, 0xE2, 0xF0, 0xF2, 0xC2][..],
            false,
            true,
            (0x1111, 0x2222, 0x7FFF_FFFF_FFFF_FFFF, u64::MAX, 0x8888),
            (
                0x8000_0000_0000_0000,
                0x2222,
                0x7FFF_FFFF_FFFF_FFFF,
                u64::MAX,
                0x8888,
                0x44,
            ),
        ),
        (
            "VEX andn ecx,ecx,edx",
            &[0xC4, 0xE2, 0x70, 0xF2, 0xCA][..],
            false,
            true,
            (0x1111, 0x2222, 0xAAAA_BBBB_FFFF_FFFF, u64::MAX, 0x8888),
            (0x1111, 0x2222, u64::from(u32::MAX), u64::MAX, 0x8888, 0x44),
        ),
        (
            "APX NF andn rax,rbx,rax",
            &[0x62, 0xF2, 0xE4, 0x0C, 0xF2, 0xC0][..],
            true,
            false,
            (u64::MAX, 0xFFFF_0000_FFFF_0000, 0x3333, 0x4444, 0x8888),
            (
                0x0000_FFFF_0000_FFFF,
                0xFFFF_0000_FFFF_0000,
                0x3333,
                0x4444,
                0x8888,
                0x45,
            ),
        ),
    ] {
        // loop: ANDN; dec r9d; jnz loop; hlt
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0x41, 0xFF, 0xC9]);
        let backedge = -i8::try_from(instruction.len() + 5).unwrap();
        code.extend_from_slice(&[0x75, backedge as u8]);
        code.push(0xF4);

        let setup = |vcpu: &mut X86_64Vcpu| {
            vcpu.set_apx_enabled(apx);
            let mut regs = vcpu.get_regs().unwrap();
            (regs.rax, regs.rbx, regs.rcx, regs.rdx, regs.r8) = initial;
            regs.r9 = 200;
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        // The direct decoder does not yet execute APX NF ANDN. Its closed-form
        // oracle below remains independent of the SMIR lifter/lowerer, while
        // legacy VEX forms additionally receive full interpreter comparison.
        let reference = direct_reference.then(|| {
            let mut interp = make_vcpu_code(&code);
            setup(&mut interp);
            run_interp(&mut interp);
            interp.get_regs().unwrap()
        });

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("JIT {name}: {error:?}")),
            "{name} loop must enter the native tier"
        );
        run_interp(&mut jit);

        let after = jit.get_regs().unwrap();
        if let Some(reference) = reference {
            assert_eq!(
                (after.rax, after.rbx, after.rcx, after.rdx, after.r8),
                (
                    reference.rax,
                    reference.rbx,
                    reference.rcx,
                    reference.rdx,
                    reference.r8,
                ),
                "{name}: registers vs interpreter"
            );
            assert_eq!(
                after.rflags & STATUS_MASK,
                reference.rflags & STATUS_MASK,
                "{name}: status flags vs interpreter"
            );
        }
        assert_eq!(
            (after.rax, after.rbx, after.rcx, after.rdx, after.r8),
            (expected.0, expected.1, expected.2, expected.3, expected.4),
            "{name}: architectural registers"
        );
        assert_eq!(after.r9 & 0xFFFF_FFFF, 0, "{name}: loop count");
        assert_eq!(
            after.rflags & STATUS_MASK,
            expected.5,
            "{name}: exact architectural status flags"
        );
    }
}

/// VEX BLSR/BLSMSK/BLSI define CF/ZF/SF/OF and preserve undefined PF/AF;
/// APX NF preserves the entire incoming status word. Running the instruction
/// inside the hot loop exercises native alias handling on every iteration.
#[test]
fn jit_bls_family_preserves_width_aliases_and_exact_flag_modes() {
    if !std::is_x86_feature_detected!("bmi1") {
        return;
    }
    const STATUS_MASK: u64 = 0x08D5;

    for (name, instruction, apx, direct_reference, initial, expected) in [
        (
            "VEX blsr rax,rbx",
            &[0xC4, 0xE2, 0xF8, 0xF3, 0xCB][..],
            false,
            true,
            (0x1111, 0x18, 0x3333, 0x4444, 0x8888),
            (0x10, 0x18, 0x3333, 0x4444, 0x8888, 0x44),
        ),
        (
            "VEX blsmsk ecx,edx",
            &[0xC4, 0xE2, 0x70, 0xF3, 0xD2][..],
            false,
            true,
            (0x1111, 0x2222, 0xAAAA_BBBB_CCCC_DDDD, 0, 0x8888),
            (0x1111, 0x2222, u64::from(u32::MAX), 0, 0x8888, 0x45),
        ),
        (
            "VEX blsi rax,rax",
            &[0xC4, 0xE2, 0xF8, 0xF3, 0xD8][..],
            false,
            true,
            (0x18, 0x2222, 0x3333, 0x4444, 0x8888),
            (0x8, 0x2222, 0x3333, 0x4444, 0x8888, 0x45),
        ),
        (
            "APX NF blsr rax,rax",
            &[0x62, 0xF2, 0xFC, 0x0C, 0xF3, 0xC8][..],
            true,
            false,
            (0x18, 0x2222, 0x3333, 0x4444, 0x8888),
            (0, 0x2222, 0x3333, 0x4444, 0x8888, 0x45),
        ),
    ] {
        // loop: BLS; dec r9d; jnz loop; hlt. DEC preserves CF, so the final
        // CF distinguishes VEX flagful execution from APX NF preservation.
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0x41, 0xFF, 0xC9]);
        let backedge = -i8::try_from(instruction.len() + 5).unwrap();
        code.extend_from_slice(&[0x75, backedge as u8]);
        code.push(0xF4);

        let setup = |vcpu: &mut X86_64Vcpu| {
            vcpu.set_apx_enabled(apx);
            let mut regs = vcpu.get_regs().unwrap();
            (regs.rax, regs.rbx, regs.rcx, regs.rdx, regs.r8) = initial;
            regs.r9 = 200;
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        let reference = direct_reference.then(|| {
            let mut interp = make_vcpu_code(&code);
            setup(&mut interp);
            run_interp(&mut interp);
            interp.get_regs().unwrap()
        });

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("JIT {name}: {error:?}")),
            "{name} loop must enter the native tier"
        );
        run_interp(&mut jit);

        let after = jit.get_regs().unwrap();
        if let Some(reference) = reference {
            assert_eq!(
                (after.rax, after.rbx, after.rcx, after.rdx, after.r8),
                (
                    reference.rax,
                    reference.rbx,
                    reference.rcx,
                    reference.rdx,
                    reference.r8,
                ),
                "{name}: registers vs interpreter"
            );
            assert_eq!(
                after.rflags & STATUS_MASK,
                reference.rflags & STATUS_MASK,
                "{name}: status flags vs interpreter"
            );
        }
        assert_eq!(
            (after.rax, after.rbx, after.rcx, after.rdx, after.r8),
            (expected.0, expected.1, expected.2, expected.3, expected.4),
            "{name}: architectural registers"
        );
        assert_eq!(after.r9 & 0xFFFF_FFFF, 0, "{name}: loop count");
        assert_eq!(
            after.rflags & STATUS_MASK,
            expected.5,
            "{name}: exact architectural status flags"
        );
    }
}

/// Register ROL/ROR stages guest RSP/RBP/APX EGPR operands and variable counts
/// through GuestRegs. The matrix covers masked zero, one, multi-bit,
/// operand-width-period, partial-write, NDD, and NF behavior without MMU or
/// semantic call helpers.
#[test]
fn jit_state_backed_gpr_rotate_execute_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        rcx: u64,
    }
    let cases = [
        Case {
            name: "ROL RSP,1",
            instruction: &[0x48, 0xD1, 0xC4],
            apx: false,
            rcx: 0,
        },
        Case {
            name: "ROR RBP,CL masked zero",
            instruction: &[0x48, 0xD3, 0xCD],
            apx: false,
            rcx: 64,
        },
        Case {
            name: "ROR RBP,CL count one",
            instruction: &[0x48, 0xD3, 0xCD],
            apx: false,
            rcx: 1,
        },
        Case {
            name: "ROR RBP,CL multi-bit",
            instruction: &[0x48, 0xD3, 0xCD],
            apx: false,
            rcx: 9,
        },
        Case {
            name: "ROL SPL,8 effective zero",
            instruction: &[0x40, 0xC0, 0xC4, 0x08],
            apx: false,
            rcx: 0,
        },
        Case {
            name: "ROR BP,17 effective one with raw multi-bit OF",
            instruction: &[0x66, 0xC1, 0xCD, 0x11],
            apx: false,
            rcx: 0,
        },
        Case {
            name: "APX NDD ROL R16,RSP,7",
            instruction: &[0x62, 0xF4, 0xFC, 0x10, 0xC1, 0xC4, 0x07],
            apx: true,
            rcx: 0,
        },
        Case {
            name: "APX NDD ROR R31,RBP,CL",
            instruction: &[0x62, 0xF4, 0x84, 0x10, 0xD3, 0xCD],
            apx: true,
            rcx: 17,
        },
        Case {
            name: "APX NF NDD ROR R31D,R16D,CL",
            instruction: &[0x62, 0xFC, 0x04, 0x14, 0xD3, 0xC8],
            apx: true,
            rcx: 1,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let setup = |vcpu: &mut X86_64Vcpu, case: &Case| {
        vcpu.set_apx_enabled(case.apx);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = case.rcx;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5681;
        regs.rbp = 0x3344_5566_8765_8001;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_8000_0011;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        regs.rflags = 0x8D7;
        vcpu.set_regs(&regs).unwrap();
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, &case);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, &case);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// Register SHL/SHR/SAR stages guest RSP/RBP/APX EGPR operands and variable
/// counts through GuestRegs. The matrix covers masked zero, one, multi-bit,
/// operand-width boundary/oversized counts, partial writes, NDD, and NF without
/// MMU or semantic call helpers.
#[test]
fn jit_state_backed_gpr_shift_execute_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        rcx: u64,
    }
    let cases = [
        Case {
            name: "SHL RSP,1",
            instruction: &[0x48, 0xD1, 0xE4],
            apx: false,
            rcx: 0,
        },
        Case {
            name: "SHR RBP,CL masked zero",
            instruction: &[0x48, 0xD3, 0xED],
            apx: false,
            rcx: 64,
        },
        Case {
            name: "SHR RBP,CL count one",
            instruction: &[0x48, 0xD3, 0xED],
            apx: false,
            rcx: 1,
        },
        Case {
            name: "SHR RBP,CL multi-bit",
            instruction: &[0x48, 0xD3, 0xED],
            apx: false,
            rcx: 9,
        },
        Case {
            name: "SHL SPL,8 boundary CF",
            instruction: &[0x40, 0xC0, 0xE4, 0x08],
            apx: false,
            rcx: 0,
        },
        Case {
            name: "SHR BP,17 oversized CF and OF",
            instruction: &[0x66, 0xC1, 0xED, 0x11],
            apx: false,
            rcx: 0,
        },
        Case {
            name: "SAR BPL,9 oversized sign CF",
            instruction: &[0x40, 0xC0, 0xFD, 0x09],
            apx: false,
            rcx: 0,
        },
        Case {
            name: "APX NDD SHL R16,RSP,7",
            instruction: &[0x62, 0xF4, 0xFC, 0x10, 0xC1, 0xE4, 0x07],
            apx: true,
            rcx: 0,
        },
        Case {
            name: "APX NDD SHR R31,RBP,CL",
            instruction: &[0x62, 0xF4, 0x84, 0x10, 0xD3, 0xED],
            apx: true,
            rcx: 17,
        },
        Case {
            name: "APX NF NDD SAR R31D,R16D,CL",
            instruction: &[0x62, 0xFC, 0x04, 0x14, 0xD3, 0xF8],
            apx: true,
            rcx: 1,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let setup = |vcpu: &mut X86_64Vcpu, case: &Case| {
        vcpu.set_apx_enabled(case.apx);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = case.rcx;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5681;
        regs.rbp = 0x3344_5566_8765_8001;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_8000_0011;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        regs.rflags = 0x8D7;
        vcpu.set_regs(&regs).unwrap();
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, &case);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, &case);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// Register RCL/RCR stages guest RSP/RBP/APX EGPR operands through GuestRegs
/// while consuming incoming CF. The matrix distinguishes the raw masked count
/// used for OF classification from the effective modulo-(width + 1) count and
/// runs without MMU or semantic call helpers.
#[test]
fn jit_state_backed_gpr_carry_rotate_execute_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        rcx: u64,
        rflags: u64,
    }
    let cases = [
        Case {
            name: "RCL RSP,1 consumes CF",
            instruction: &[0x48, 0xD1, 0xD4],
            apx: false,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "RCR RBP,CL masked zero",
            instruction: &[0x48, 0xD3, 0xDD],
            apx: false,
            rcx: 64,
            rflags: 0x8D7,
        },
        Case {
            name: "RCR RBP,CL count one",
            instruction: &[0x48, 0xD3, 0xDD],
            apx: false,
            rcx: 1,
            rflags: 0x8D7,
        },
        Case {
            name: "RCR RBP,CL multi-bit",
            instruction: &[0x48, 0xD3, 0xDD],
            apx: false,
            rcx: 9,
            rflags: 0x8D7,
        },
        Case {
            name: "RCL SPL,9 full period",
            instruction: &[0x40, 0xC0, 0xD4, 0x09],
            apx: false,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "RCR BP,18 effective one with raw multi-bit OF",
            instruction: &[0x66, 0xC1, 0xDD, 0x12],
            apx: false,
            rcx: 0,
            rflags: 0x0D7,
        },
        Case {
            name: "APX NDD RCL R16,RSP,1",
            instruction: &[0x62, 0xF4, 0xFC, 0x10, 0xD1, 0xD4],
            apx: true,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "APX NDD RCR R31,RBP,CL",
            instruction: &[0x62, 0xF4, 0x84, 0x10, 0xD3, 0xDD],
            apx: true,
            rcx: 17,
            rflags: 0x8D7,
        },
        Case {
            name: "APX NDD RCL R16B,RSPB,10 raw multi-bit OF",
            instruction: &[0x62, 0xF4, 0x7C, 0x10, 0xC0, 0xD4, 0x0A],
            apx: true,
            rcx: 0,
            rflags: 0x0D7,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let setup = |vcpu: &mut X86_64Vcpu, case: &Case| {
        vcpu.set_apx_enabled(case.apx);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = case.rcx;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5681;
        regs.rbp = 0x3344_5566_8765_8001;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_8000_0011;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        regs.rflags = case.rflags;
        vcpu.set_regs(&regs).unwrap();
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, &case);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, &case);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// Destructive and APX NDD SHLD/SHRD stage guest RSP/RBP/APX EGPR operands and
/// CL through GuestRegs. The matrix covers zero, one, multi-bit, W16
/// boundary/undefined counts, independent NDD destinations, REX2, and APX NF
/// without MMU or semantic call helpers.
#[test]
fn jit_state_backed_gpr_double_shift_execute_without_memory_helpers() {
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        apx: bool,
        rcx: u64,
    }
    let cases = [
        Case {
            name: "SHLD RSP,RBP,1",
            instruction: &[0x48, 0x0F, 0xA4, 0xEC, 0x01],
            apx: false,
            rcx: 0,
        },
        Case {
            name: "SHRD RBP,RSP,CL masked zero",
            instruction: &[0x48, 0x0F, 0xAD, 0xE5],
            apx: false,
            rcx: 64,
        },
        Case {
            name: "SHRD RBP,RSP,CL count one",
            instruction: &[0x48, 0x0F, 0xAD, 0xE5],
            apx: false,
            rcx: 1,
        },
        Case {
            name: "SHRD RBP,RSP,CL multi-bit",
            instruction: &[0x48, 0x0F, 0xAD, 0xE5],
            apx: false,
            rcx: 9,
        },
        Case {
            name: "SHLD BP,SP,17 undefined no-op",
            instruction: &[0x66, 0x0F, 0xA4, 0xE5, 0x11],
            apx: false,
            rcx: 0,
        },
        Case {
            name: "SHLD AX,BX,17 low-register deterministic no-op",
            instruction: &[0x66, 0x0F, 0xA4, 0xD8, 0x11],
            apx: false,
            rcx: 0,
        },
        Case {
            name: "SHRD AX,BX,CL low-register dynamic deterministic no-op",
            instruction: &[0x66, 0x0F, 0xAD, 0xD8],
            apx: false,
            rcx: 17,
        },
        Case {
            name: "SHRD BP,SP,16 width boundary",
            instruction: &[0x66, 0x0F, 0xAC, 0xE5, 0x10],
            apx: false,
            rcx: 0,
        },
        Case {
            name: "REX2 SHLD RSP,R16,1",
            instruction: &[0xD5, 0xC8, 0xA4, 0xC4, 0x01],
            apx: true,
            rcx: 0,
        },
        Case {
            name: "REX2 SHRD R16,R31,CL",
            instruction: &[0xD5, 0xDC, 0xAD, 0xF8],
            apx: true,
            rcx: 9,
        },
        Case {
            name: "APX NF SHRD RSP,R31,4",
            instruction: &[0x62, 0x64, 0xFC, 0x0C, 0x2C, 0xFC, 0x04],
            apx: true,
            rcx: 0,
        },
        Case {
            name: "APX NDD SHLD R16,RSP,R31,4",
            instruction: &[0x62, 0x64, 0xFC, 0x10, 0x24, 0xFC, 0x04],
            apx: true,
            rcx: 0,
        },
        Case {
            name: "APX NDD SHRD RSP,RBP,R31,CL",
            instruction: &[0x62, 0x64, 0xDC, 0x18, 0xAD, 0xFD],
            apx: true,
            rcx: 9,
        },
        Case {
            name: "APX NDD SHRD R16W,BP,R31W,CL dynamic deterministic base copy",
            instruction: &[0x62, 0x64, 0x7D, 0x10, 0xAD, 0xFD],
            apx: true,
            rcx: 17,
        },
        Case {
            name: "APX NF NDD SHLD R31,RBP,RSP,4",
            instruction: &[0x62, 0xF4, 0x84, 0x14, 0x24, 0xE5, 0x04],
            apx: true,
            rcx: 0,
        },
        Case {
            name: "APX NDD SHLD DX,AX,BX,17 deterministic base copy",
            instruction: &[0x62, 0xF4, 0x6D, 0x18, 0x24, 0xD8, 0x11],
            apx: true,
            rcx: 0,
        },
        Case {
            name: "APX NDD SHRD DX,AX,BX,CL dynamic deterministic base copy",
            instruction: &[0x62, 0xF4, 0x6D, 0x18, 0xAD, 0xD8],
            apx: true,
            rcx: 17,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let setup = |vcpu: &mut X86_64Vcpu, case: &Case| {
        vcpu.set_apx_enabled(case.apx);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = case.rcx;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5681;
        regs.rbp = 0x3344_5566_8765_8001;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_8000_0011;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        regs.rflags = 0x8D7;
        vcpu.set_regs(&regs).unwrap();
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, &case);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, &case);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// Register ADCX/ADOX stages guest RSP/RBP operands through GuestRegs while
/// consuming the incoming CF/OF, updating only the selected output bit, and
/// retaining destructive aliases and dword zero-extension.
#[test]
fn jit_state_backed_gpr_adx_execute_without_memory_helpers() {
    if !std::is_x86_feature_detected!("adx") {
        return;
    }
    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        dst_index: usize,
        src2_index: usize,
        dst_value: u64,
        src2_value: u64,
        status: u64,
        dword: bool,
    }
    let cases = [
        Case {
            name: "ADCX RSP,RBP consumes set CF",
            instruction: &[0x66, 0x48, 0x0F, 0x38, 0xF6, 0xE5],
            dst_index: 4,
            src2_index: 5,
            dst_value: u64::MAX,
            src2_value: 0,
            status: 0x8D5,
            dword: false,
        },
        Case {
            name: "ADOX RBP,RSP consumes set OF",
            instruction: &[0xF3, 0x48, 0x0F, 0x38, 0xF6, 0xEC],
            dst_index: 5,
            src2_index: 4,
            dst_value: u64::MAX,
            src2_value: 0,
            status: 0x8D5,
            dword: false,
        },
        Case {
            name: "ADCX R8D,ESP zero-extends",
            instruction: &[0x66, 0x44, 0x0F, 0x38, 0xF6, 0xC4],
            dst_index: 8,
            src2_index: 4,
            dst_value: 0xAABB_CCDD_FFFF_FFFF,
            src2_value: 0x1122_3344_FFFF_FFFF,
            status: 0x8D4,
            dword: true,
        },
        Case {
            name: "ADOX EBP,EBP all operands alias",
            instruction: &[0xF3, 0x0F, 0x38, 0xF6, 0xED],
            dst_index: 5,
            src2_index: 5,
            dst_value: 0xDEAD_BEEF_7FFF_FFFF,
            src2_value: 0xDEAD_BEEF_7FFF_FFFF,
            status: 0x8D5,
            dword: true,
        },
    ];

    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    let set_gpr = |regs: &mut Registers, index: usize, value: u64| match index {
        4 => regs.rsp = value,
        5 => regs.rbp = value,
        8 => regs.r8 = value,
        _ => unreachable!(),
    };
    let setup = |vcpu: &mut X86_64Vcpu, case: &Case| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x0102_0304_0506_0708;
        regs.rcx = 0x1122_3344_5566_1234;
        regs.rdx = 0x99AA_BBCC_DDEE_FF00;
        regs.rbx = 0x0F1E_2D3C_4B5A_6978;
        regs.rsp = 0x2233_4455_6677_5678;
        regs.rbp = 0x3344_5566_8765_9ABC;
        regs.r8 = 0x8899_AABB_CCDD_EEFF;
        regs.r16 = 0xAABB_CCDD_EEFF_7788;
        regs.r31 = 0xFFEE_DDCC_BBAA_1357;
        set_gpr(&mut regs, case.dst_index, case.dst_value);
        set_gpr(&mut regs, case.src2_index, case.src2_value);
        regs.rflags = 0x2 | case.status;
        vcpu.set_regs(&regs).unwrap();
    };

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp, &case);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        if case.dword {
            assert_eq!(
                gprs(&expected)[case.dst_index] >> 32,
                0,
                "{} reference result must zero-extend",
                case.name
            );
        }

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit, &case);
        jit.set_jit_call(false);
        jit.set_jit_mem(false);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the register-only native tier:\n{}",
            case.name,
            jit.jit_dump_region(LOAD_ADDR)
        );
        let actual = jit.get_regs().unwrap();
        assert_eq!(gprs(&actual), gprs(&expected), "{} GPR file", case.name);
        assert_eq!(actual.rflags, expected.rflags, "{} RFLAGS", case.name);
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
    }
}

/// ADX carry chains read and update only CF (ADCX) or OF (ADOX). The loop and
/// split-final-block shapes force native-region execution while retaining exact
/// final flags, and APX NDD additionally verifies independent source/destination
/// operands survive the two-operand host encoding.
#[test]
fn jit_adcx_adox_preserve_carry_chains_flags_and_apx_ndd_sources() {
    if !std::is_x86_feature_detected!("adx") {
        return;
    }
    const STATUS_MASK: u64 = 0x08D5;

    for (name, instruction, apx, initial, expected) in [
        (
            "legacy adcx rax,rbx",
            &[0x66, 0x48, 0x0F, 0x38, 0xF6, 0xC3][..],
            false,
            (u64::MAX, 0, 0xA5A5, 0xCD7),
            (0, 0, 0xA5A5, 0x45),
        ),
        (
            "APX NDD adcx r8,rax,rbx",
            &[0x62, 0xF4, 0xBD, 0x18, 0x66, 0xC3][..],
            true,
            (u64::MAX, 0, 0xDEAD, 0xCD7),
            (u64::MAX, 0, 0, 0x45),
        ),
    ] {
        // ADX; dec r9d; jnz ADX; hlt. r9d=1 executes ADX once; DEC then
        // materializes deterministic PF/ZF while preserving ADCX's CF.
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0x41, 0xFF, 0xC9]);
        let backedge = -i8::try_from(instruction.len() + 5).unwrap();
        code.extend_from_slice(&[0x75, backedge as u8, 0xF4]);

        let setup = |vcpu: &mut X86_64Vcpu| {
            vcpu.set_apx_enabled(apx);
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = initial.0;
            regs.rbx = initial.1;
            regs.r8 = initial.2;
            regs.r9 = 1;
            regs.rflags = initial.3;
            vcpu.set_regs(&regs).unwrap();
        };

        let reference = (!apx).then(|| {
            let mut interp = make_vcpu_code(&code);
            setup(&mut interp);
            run_interp(&mut interp);
            interp.get_regs().unwrap()
        });

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("JIT {name}: {error:?}")),
            "{name} loop must enter the native tier"
        );
        run_interp(&mut jit);
        let after = jit.get_regs().unwrap();

        if let Some(reference) = reference {
            assert_eq!(after.rax, reference.rax, "{name}: rax vs interpreter");
            assert_eq!(after.rbx, reference.rbx, "{name}: rbx vs interpreter");
            assert_eq!(
                after.rflags & STATUS_MASK,
                reference.rflags & STATUS_MASK,
                "{name}: status vs interpreter"
            );
        }
        assert_eq!(
            after.rax, expected.0,
            "{name}: source 1 / legacy destination"
        );
        assert_eq!(after.rbx, expected.1, "{name}: source 2 preserved");
        assert_eq!(after.r8, expected.2, "{name}: APX destination");
        assert_eq!(after.r9 & u64::from(u32::MAX), 0, "{name}: loop count");
        assert_eq!(
            after.rflags & STATUS_MASK,
            expected.3,
            "{name}: exact status"
        );
    }

    // mov r10d,0x80000000; add r10d,r10d; adox rax,rbx; jnz loop; hlt.
    // The ADD supplies OF=1 and ZF=1. ADOX consumes OF, produces 9 with
    // OF=0, and preserves ZF so JNZ exits after one native iteration. The
    // syntactic backedge makes this a stable hot region on every host.
    let code = [
        0x41, 0xBA, 0x00, 0x00, 0x00, 0x80, 0x45, 0x01, 0xD2, 0xF3, 0x48, 0x0F, 0x38, 0xF6, 0xC3,
        0x75, 0xEF, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 5;
        regs.rbx = 3;
        regs.r10 = 0;
        regs.rflags = 0x2;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interp = make_vcpu_code(&code);
    setup(&mut interp);
    run_interp(&mut interp);
    let reference = interp.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block().expect("JIT ADOX loop"),
        "ADOX loop must enter the native tier"
    );
    run_interp(&mut jit);
    let after = jit.get_regs().unwrap();
    assert_eq!(after.rax, 9);
    assert_eq!(after.rbx, 3);
    assert_eq!(after.r10, 0);
    assert_eq!(after.rflags & STATUS_MASK, 0x45);
    assert_eq!(after.rax, reference.rax);
    assert_eq!(after.r10, reference.r10);
    assert_eq!(after.rflags & STATUS_MASK, reference.rflags & STATUS_MASK);
}

/// Register-only BMI1/BMI2 operations are native-JIT eligible at both scalar
/// widths. The matrix covers zero extension, boundary counts, defined versus
/// preserved flags, and destination aliasing with every explicit source role.
#[test]
fn jit_bmi_bit_extract_deposit_preserves_semantics_flags_and_aliases() {
    const STATUS_MASK: u64 = 0x08D5;
    let bmi1 = std::is_x86_feature_detected!("bmi1");
    let bmi2 = std::is_x86_feature_detected!("bmi2");

    for (
        name,
        instruction,
        supported,
        rax,
        rcx,
        rdx,
        expected_rax,
        expected_rcx,
        expected_rdx,
        expected_status,
    ) in [
        (
            "bextr eax,edx,ecx",
            &[0xC4, 0xE2, 0x70, 0xF7, 0xC2][..],
            bmi1,
            u64::MAX,
            (8 << 8) | 4,
            0xAABB_CCDD_0000_F0F0,
            0x0F,
            (8 << 8) | 4,
            0xAABB_CCDD_0000_F0F0,
            0x04,
        ),
        (
            "bextr rax,rdx,rcx",
            &[0xC4, 0xE2, 0xF0, 0xF7, 0xC2][..],
            bmi1,
            u64::MAX,
            (8 << 8) | 60,
            0xF123_4567_89AB_CDEF,
            0x0F,
            (8 << 8) | 60,
            0xF123_4567_89AB_CDEF,
            0x04,
        ),
        (
            "bextr rcx,rdx,rcx (dst=control)",
            &[0xC4, 0xE2, 0xF0, 0xF7, 0xCA][..],
            bmi1,
            0x1122_3344_5566_7788,
            (16 << 8) | 8,
            0xAABB_CCDD_EEFF_1234,
            0x1122_3344_5566_7788,
            0xFF12,
            0xAABB_CCDD_EEFF_1234,
            0x04,
        ),
        (
            "bzhi eax,edx,ecx",
            &[0xC4, 0xE2, 0x70, 0xF5, 0xC2][..],
            bmi2,
            u64::MAX,
            12,
            0xFEDC_BA98_7654_3210,
            0x210,
            12,
            0xFEDC_BA98_7654_3210,
            0x04,
        ),
        (
            "bzhi rax,rdx,rcx",
            &[0xC4, 0xE2, 0xF0, 0xF5, 0xC2][..],
            bmi2,
            u64::MAX,
            64,
            0xFEDC_BA98_7654_3210,
            0xFEDC_BA98_7654_3210,
            64,
            0xFEDC_BA98_7654_3210,
            0x85,
        ),
        (
            "bzhi rdx,rdx,rcx (dst=src)",
            &[0xC4, 0xE2, 0xF0, 0xF5, 0xD2][..],
            bmi2,
            0x1122_3344_5566_7788,
            20,
            0xAABB_CCDD_EEFF_1234,
            0x1122_3344_5566_7788,
            20,
            0xF_1234,
            0x04,
        ),
        (
            "pdep eax,ecx,edx",
            &[0xC4, 0xE2, 0x73, 0xF5, 0xC2][..],
            bmi2,
            u64::MAX,
            0x0B,
            0x55,
            0x45,
            0x0B,
            0x55,
            0x45,
        ),
        (
            "pdep rax,rcx,rdx",
            &[0xC4, 0xE2, 0xF3, 0xF5, 0xC2][..],
            bmi2,
            u64::MAX,
            0x1B,
            0x8000_0000_0000_0055,
            0x8000_0000_0000_0045,
            0x1B,
            0x8000_0000_0000_0055,
            0x45,
        ),
        (
            "pdep rcx,rcx,rdx (dst=src)",
            &[0xC4, 0xE2, 0xF3, 0xF5, 0xCA][..],
            bmi2,
            0x1122_3344_5566_7788,
            0x0B,
            0x55,
            0x1122_3344_5566_7788,
            0x45,
            0x55,
            0x45,
        ),
        (
            "pext eax,ecx,edx",
            &[0xC4, 0xE2, 0x72, 0xF5, 0xC2][..],
            bmi2,
            u64::MAX,
            0x45,
            0x55,
            0x0B,
            0x45,
            0x55,
            0x45,
        ),
        (
            "pext rax,rcx,rdx",
            &[0xC4, 0xE2, 0xF2, 0xF5, 0xC2][..],
            bmi2,
            u64::MAX,
            0x8000_0000_0000_0045,
            0x8000_0000_0000_0055,
            0x1B,
            0x8000_0000_0000_0045,
            0x8000_0000_0000_0055,
            0x45,
        ),
        (
            "pext rdx,rcx,rdx (dst=mask)",
            &[0xC4, 0xE2, 0xF2, 0xF5, 0xD2][..],
            bmi2,
            0x1122_3344_5566_7788,
            0x45,
            0x55,
            0x1122_3344_5566_7788,
            0x45,
            0x0B,
            0x45,
        ),
    ] {
        if !supported {
            continue;
        }

        // loop: dec r8d; jnz loop
        //       <BMI operation>; hlt
        // The terminal DEC establishes CF|PF|ZF before BMI. BEXTR/BZHI merge
        // their defined outputs while preserving undefined PF/AF; PDEP/PEXT
        // preserve the complete status word.
        let mut code = vec![0x41, 0xFF, 0xC8, 0x75, 0xFB];
        code.extend_from_slice(instruction);
        code.push(0xF4);

        let setup = |vcpu: &mut X86_64Vcpu| {
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = rax;
            regs.rcx = rcx;
            regs.rdx = rdx;
            regs.r8 = 200;
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp);
        run_interp(&mut interp);

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("JIT {name}: {error:?}")),
            "{name} loop must enter the native tier"
        );
        run_interp(&mut jit);

        let expected = interp.get_regs().unwrap();
        let after = jit.get_regs().unwrap();
        assert_eq!(after.rax, expected.rax, "{name}: RAX vs interpreter");
        assert_eq!(after.rcx, expected.rcx, "{name}: RCX vs interpreter");
        assert_eq!(after.rdx, expected.rdx, "{name}: RDX vs interpreter");
        assert_eq!(after.rax, expected_rax, "{name}: architectural RAX");
        assert_eq!(after.rcx, expected_rcx, "{name}: architectural RCX");
        assert_eq!(after.rdx, expected_rdx, "{name}: architectural RDX");
        assert_eq!(after.r8 & 0xFFFF_FFFF, 0, "{name}: loop count");
        assert_eq!(
            after.rflags & STATUS_MASK,
            expected.rflags & STATUS_MASK,
            "{name}: status flags vs interpreter"
        );
        assert_eq!(
            after.rflags & STATUS_MASK,
            expected_status,
            "{name}: architectural status flags"
        );
    }
}

/// APX NDD carry operations whose destination aliases source 2 must remain in
/// the native tier. ADC can commute its register sources; SBB preserves the old
/// source 2 on the host stack without disturbing incoming or result flags.
#[test]
fn jit_apx_ndd_adc_sbb_alias_second_source_preserves_carry_semantics() {
    const STATUS_MASK: u64 = 0x0ED5;
    for (name, instruction, rax, r8, expected_r8, expected_status) in [
        (
            "adc",
            &[0x62, 0x74, 0xBC, 0x18, 0x11, 0xC0][..],
            1,
            u64::MAX,
            201,
            0x44,
        ),
        (
            "sbb",
            &[0x62, 0x74, 0xBC, 0x18, 0x19, 0xC0][..],
            0,
            1,
            1,
            0x45,
        ),
    ] {
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0xFF, 0xC9]); // dec ecx (preserves CF)
        code.extend_from_slice(&[0x75, 0xF6]); // jnz to APX instruction
        code.push(0xF4);

        let setup = |vcpu: &mut X86_64Vcpu| {
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = rax;
            regs.r8 = r8;
            regs.rcx = 200;
            regs.rflags = 0x3; // architectural bit 1 + incoming CF
            vcpu.set_regs(&regs).unwrap();
        };

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("jit APX NDD {name} alias: {error:?}")),
            "APX NDD {name} alias loop must enter the native tier"
        );
        run_interp(&mut jit);
        let jit_regs = jit.get_regs().unwrap();

        assert_eq!(jit_regs.rax, rax, "{name}: source 1 must be preserved");
        assert_eq!(jit_regs.r8, expected_r8, "{name}: closed-form result");
        assert_eq!(jit_regs.rcx & 0xFFFF_FFFF, 0, "{name}: loop count");
        assert_eq!(
            jit_regs.rflags & STATUS_MASK,
            expected_status,
            "{name}: final DEC flags plus carry/borrow from the NDD operation"
        );
    }
}

#[test]
fn jit_apx_ndd_binary_alu_alias_second_source_preserves_full_semantics() {
    const STATUS_MASK: u64 = 0x0ED5;
    for (name, instruction, rax, r8, expected_r8) in [
        ("add", &[0x62, 0x74, 0xBC, 0x18, 0x01, 0xC0][..], 3, 5, 605),
        (
            "or",
            &[0x62, 0x74, 0xBC, 0x18, 0x09, 0xC0][..],
            0xF0F0,
            0x000F,
            0xF0FF,
        ),
        (
            "and",
            &[0x62, 0x74, 0xBC, 0x18, 0x21, 0xC0][..],
            0xFF00,
            0xF0F0,
            0xF000,
        ),
        ("sub", &[0x62, 0x74, 0xBC, 0x18, 0x29, 0xC0][..], 10, 3, 3),
        (
            "xor",
            &[0x62, 0x74, 0xBC, 0x18, 0x31, 0xC0][..],
            0xA5A5,
            0x1234,
            0x1234,
        ),
    ] {
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0xFF, 0xC9]); // dec ecx (preserves CF)
        code.extend_from_slice(&[0x75, 0xF6]); // jnz to APX instruction
        code.push(0xF4);

        let mut jit = make_vcpu_code(&code);
        let mut regs = jit.get_regs().unwrap();
        regs.rax = rax;
        regs.r8 = r8;
        regs.rcx = 200;
        regs.rflags = 0x2;
        jit.set_regs(&regs).unwrap();
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("jit APX NDD {name} alias: {error:?}")),
            "APX NDD {name} alias loop must enter the native tier"
        );
        run_interp(&mut jit);
        let after = jit.get_regs().unwrap();

        assert_eq!(after.rax, rax, "{name}: source 1 must be preserved");
        assert_eq!(after.r8, expected_r8, "{name}: closed-form result");
        assert_eq!(after.rcx & 0xFFFF_FFFF, 0, "{name}: loop count");
        assert_eq!(
            after.rflags & STATUS_MASK,
            0x44,
            "{name}: final DEC flags and operation carry"
        );
    }
}

#[test]
fn jit_apx_nf_binary_alu_alias_second_source_preserves_all_flags() {
    const STATUS_MASK: u64 = 0x0ED5;
    for (name, instruction, rax, r8, expected_r8) in [
        ("add", &[0x62, 0x74, 0xBC, 0x1C, 0x01, 0xC0][..], 3, 5, 605),
        (
            "or",
            &[0x62, 0x74, 0xBC, 0x1C, 0x09, 0xC0][..],
            0xF0F0,
            0x000F,
            0xF0FF,
        ),
        (
            "and",
            &[0x62, 0x74, 0xBC, 0x1C, 0x21, 0xC0][..],
            0xFF00,
            0xF0F0,
            0xF000,
        ),
        ("sub", &[0x62, 0x74, 0xBC, 0x1C, 0x29, 0xC0][..], 10, 3, 3),
        (
            "xor",
            &[0x62, 0x74, 0xBC, 0x1C, 0x31, 0xC0][..],
            0xA5A5,
            0x1234,
            0x1234,
        ),
    ] {
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0xFF, 0xC9]); // dec ecx (preserves CF)
        code.extend_from_slice(&[0x75, 0xF6]); // jnz to APX instruction
        code.push(0xF4);

        let mut jit = make_vcpu_code(&code);
        let mut regs = jit.get_regs().unwrap();
        regs.rax = rax;
        regs.r8 = r8;
        regs.rcx = 200;
        regs.rflags = 0x3;
        jit.set_regs(&regs).unwrap();
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("jit APX NF {name} alias: {error:?}")),
            "APX NF {name} alias loop must enter the native tier"
        );
        run_interp(&mut jit);
        let after = jit.get_regs().unwrap();

        assert_eq!(after.rax, rax, "{name}: source 1");
        assert_eq!(after.r8, expected_r8, "{name}: closed-form result");
        assert_eq!(after.rcx & 0xFFFF_FFFF, 0, "{name}: loop count");
        assert_eq!(
            after.rflags & STATUS_MASK,
            0x45,
            "{name}: NF must preserve CF while DEC defines the other status flags"
        );
    }
}

#[test]
fn jit_apx_ndd_imul_alias_second_source_preserves_product_and_nf_flags() {
    const STATUS_MASK: u64 = 0x0ED5;
    for (name, instruction, expected_status) in [
        ("imul", &[0x62, 0xF4, 0xE4, 0x18, 0xAF, 0xC3][..], 0x44),
        ("{nf} imul", &[0x62, 0xF4, 0xE4, 0x1C, 0xAF, 0xC3][..], 0x45),
    ] {
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0xFF, 0xC9]); // dec ecx (preserves CF)
        code.extend_from_slice(&[0x75, 0xF6]); // jnz to APX instruction
        code.push(0xF4);

        let mut jit = make_vcpu_code(&code);
        let mut regs = jit.get_regs().unwrap();
        regs.rax = 1;
        regs.rbx = 7;
        regs.rcx = 200;
        regs.rflags = 0x3; // incoming CF distinguishes regular IMUL from NF
        jit.set_regs(&regs).unwrap();
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("jit APX NDD {name} alias: {error:?}")),
            "APX NDD {name} alias loop must enter the native tier"
        );
        run_interp(&mut jit);
        let after = jit.get_regs().unwrap();

        assert_eq!(after.rax, 1, "{name}: source 1 must be preserved");
        assert_eq!(after.rbx, 7, "{name}: exact non-overflowing product");
        assert_eq!(after.rcx & 0xFFFF_FFFF, 0, "{name}: loop count");
        assert_eq!(
            after.rflags & STATUS_MASK,
            expected_status,
            "{name}: NF must preserve incoming CF while regular IMUL clears it"
        );
    }
}

/// BMI2 MULX must remain non-destructive, preserve flags for the following
/// branch, and commit the upper half when both destinations alias. The cases
/// cover distinct destinations, implicit-RDX aliasing, explicit-source
/// aliasing, same-destination ordering, and 32-bit zero extension.
#[test]
fn jit_bmi2_mulx_preserves_flags_and_all_register_aliases() {
    if !std::is_x86_feature_detected!("bmi2") {
        return;
    }

    let multiplicand = 0xFEDC_BA98_7654_3211u64;
    let multiplier = 0x1234_5678_9ABC_DEF3u64;
    let product = (multiplicand as u128) * (multiplier as u128);
    let lo = product as u64;
    let hi = (product >> 64) as u64;
    let product32 = ((multiplicand as u32 as u64) * (multiplier as u32 as u64)) as u64;
    let lo32 = product32 & 0xFFFF_FFFF;
    let hi32 = product32 >> 32;

    for (name, instruction, expected_rax, expected_rbx, expected_rdx, expected_rsi) in [
        (
            "distinct",
            &[0xC4, 0xE2, 0xCB, 0xF6, 0xD8][..], // mulx rbx,rsi,rax
            multiplier,
            hi,
            multiplicand,
            lo,
        ),
        (
            "implicit source aliases high destination",
            &[0xC4, 0xE2, 0xE3, 0xF6, 0xD0][..], // mulx rdx,rbx,rax
            multiplier,
            lo,
            hi,
            0,
        ),
        (
            "explicit source aliases low destination",
            &[0xC4, 0xE2, 0xFB, 0xF6, 0xD8][..], // mulx rbx,rax,rax
            lo,
            hi,
            multiplicand,
            0,
        ),
        (
            "same destination keeps upper half",
            &[0xC4, 0xE2, 0xE3, 0xF6, 0xD8][..], // mulx rbx,rbx,rax
            multiplier,
            hi,
            multiplicand,
            0,
        ),
        (
            "32-bit destinations zero extend",
            &[0xC4, 0xE2, 0x4B, 0xF6, 0xD8][..], // mulx ebx,esi,eax
            multiplier,
            hi32,
            multiplicand,
            lo32,
        ),
    ] {
        // test r8,r8 sets ZF; MULX; jnz fail; dec rcx; jnz loop; hlt;
        // fail: mov edi,1; hlt. A flag-clobbering MULX takes the fail path.
        let mut code = vec![0x4D, 0x85, 0xC0];
        code.extend_from_slice(instruction);
        code.extend_from_slice(&[
            0x75, 0x06, 0x48, 0xFF, 0xC9, 0x75, 0xF1, 0xF4, 0xBF, 0x01, 0x00, 0x00, 0x00, 0xF4,
        ]);

        let setup = |vcpu: &mut X86_64Vcpu| {
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = multiplier;
            regs.rbx = 0;
            regs.rcx = 1;
            regs.rdx = multiplicand;
            regs.rsi = 0;
            regs.rdi = 0;
            regs.r8 = 0;
            regs.rflags = 0x2 | 0x0ED5;
            vcpu.set_regs(&regs).unwrap();
        };

        let mut interp = make_vcpu_code(&code);
        setup(&mut interp);
        run_interp(&mut interp);

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("jit BMI2 MULX {name}: {error:?}")),
            "{name}: MULX loop must enter the native tier"
        );
        run_interp(&mut jit);

        let expected = interp.get_regs().unwrap();
        let after = jit.get_regs().unwrap();
        assert_eq!(after.rax, expected.rax, "{name}: rax vs interpreter");
        assert_eq!(after.rbx, expected.rbx, "{name}: rbx vs interpreter");
        assert_eq!(after.rdx, expected.rdx, "{name}: rdx vs interpreter");
        assert_eq!(after.rsi, expected.rsi, "{name}: rsi vs interpreter");
        assert_eq!(after.rdi, 0, "{name}: preserved ZF must avoid fail path");
        assert_eq!(after.rax, expected_rax, "{name}: closed-form rax");
        assert_eq!(after.rbx, expected_rbx, "{name}: closed-form rbx");
        assert_eq!(after.rdx, expected_rdx, "{name}: closed-form rdx");
        assert_eq!(after.rsi, expected_rsi, "{name}: closed-form rsi");
    }
}

/// Legacy AH/BH sources cannot be named by any REX-prefixed MOVX encoding.
/// The JIT therefore snapshots the full parent register and extends byte 1
/// from the host stack. Source/destination aliasing and flag-dependent control
/// flow exercise the semantic hazards that caused the former deoptimization.
#[test]
fn jit_legacy_high_byte_movx_preserves_aliases_and_flags() {
    // test r8,r8; movzx eax,ah; movsx ebx,bh; jnz fail;
    // dec ecx; jnz loop; hlt; fail: mov edx,1; hlt
    let code = [
        0x4D, 0x85, 0xC0, 0x0F, 0xB6, 0xC4, 0x0F, 0xBE, 0xDF, 0x75, 0x05, 0xFF, 0xC9, 0x75, 0xF1,
        0xF4, 0xBA, 0x01, 0x00, 0x00, 0x00, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0x1122_3344_5566_A5CD;
        regs.rbx = 0x8877_6655_4433_80EF;
        regs.rcx = 1;
        regs.rdx = 0;
        regs.r8 = 0;
        regs.rflags = 0x2 | 0x0ED5;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interp = make_vcpu_code(&code);
    setup(&mut interp);
    run_interp(&mut interp);

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("jit legacy high-byte MOVZX/MOVSX"),
        "legacy high-byte MOVX loop must enter the native tier"
    );
    run_interp(&mut jit);

    let expected = interp.get_regs().unwrap();
    let after = jit.get_regs().unwrap();
    assert_eq!(after.rax, expected.rax, "MOVZX EAX,AH vs interpreter");
    assert_eq!(after.rbx, expected.rbx, "MOVSX EBX,BH vs interpreter");
    assert_eq!(after.rdx, 0, "preserved ZF must avoid the fail path");
    assert_eq!(after.rax, 0xA5, "MOVZX source/destination alias");
    assert_eq!(after.rbx, 0xFFFF_FF80, "MOVSX source/destination alias");
}

#[test]
fn jit_apx_ndd_double_shift_handles_fill_cl_aliases_and_nf() {
    const STATUS_MASK: u64 = 0x0ED5;
    let base = 0xF123_4567_89AB_CDEFu64;
    let initial_fill = 0x0FED_CBA9_8765_4321u64;
    for (name, instruction, expected_status) in [
        (
            "shld fill alias",
            &[0x62, 0xF4, 0xE4, 0x18, 0x24, 0xD8, 0x04][..],
            0x45,
        ),
        (
            "{nf} shld fill alias",
            &[0x62, 0xF4, 0xE4, 0x1C, 0x24, 0xD8, 0x04][..],
            0x44,
        ),
    ] {
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0x41, 0xFF, 0xC9]); // dec r9d (preserves CF)
        code.extend_from_slice(&[0x75, 0xF4]); // jnz to seven-byte APX instruction
        code.push(0xF4);

        let mut expected = initial_fill;
        for _ in 0..200 {
            expected = (base << 4) | (expected >> 60);
        }
        let mut jit = make_vcpu_code(&code);
        let mut regs = jit.get_regs().unwrap();
        regs.rax = base;
        regs.rbx = initial_fill;
        regs.r9 = 200;
        regs.rflags = 0x2;
        jit.set_regs(&regs).unwrap();
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("jit APX NDD {name}: {error:?}")),
            "APX NDD {name} loop must enter the native tier"
        );
        run_interp(&mut jit);
        let after = jit.get_regs().unwrap();
        assert_eq!(after.rax, base, "{name}: base source");
        assert_eq!(after.rbx, expected, "{name}: aliased fill/result");
        assert_eq!(after.r9 & 0xFFFF_FFFF, 0, "{name}: loop count");
        assert_eq!(
            after.rflags & STATUS_MASK,
            expected_status,
            "{name}: regular CF versus NF-preserved CF"
        );
    }

    // Choose base bits 4..11 = 4 so the SHRD result retains CL=4 on every
    // iteration even though RCX is both the count and destination.
    let base = 0x0123_4567_89AB_C040u64;
    let fill = 0xFEDC_BA98_7654_3210u64;
    let expected = (base >> 4) | (fill << 60);
    let mut code = vec![0x62, 0xF4, 0xF4, 0x18, 0xAD, 0xD8];
    code.extend_from_slice(&[0x41, 0xFF, 0xC9]); // dec r9d
    code.extend_from_slice(&[0x75, 0xF5]); // jnz to six-byte APX instruction
    code.push(0xF4);
    let mut jit = make_vcpu_code(&code);
    let mut regs = jit.get_regs().unwrap();
    regs.rax = base;
    regs.rbx = fill;
    regs.rcx = 4;
    regs.r9 = 200;
    regs.rflags = 0x2;
    jit.set_regs(&regs).unwrap();
    assert!(
        jit.jit_try_block()
            .expect("jit APX NDD SHRD destination/CL alias"),
        "APX NDD SHRD destination/CL alias loop must enter the native tier"
    );
    run_interp(&mut jit);
    let after = jit.get_regs().unwrap();
    assert_eq!(after.rax, base);
    assert_eq!(after.rbx, fill);
    assert_eq!(after.rcx, expected);
    assert_eq!(after.r9 & 0xFFFF_FFFF, 0);
    assert_eq!(after.rflags & STATUS_MASK, 0x44);
}

#[test]
fn jit_apx_ndd_single_shift_destination_cl_alias_preserves_count_and_nf() {
    const STATUS_MASK: u64 = 0x0ED5;
    for (name, instruction, expected_status) in [
        ("shl", &[0x62, 0xF4, 0xF4, 0x18, 0xD3, 0xE0][..], 0x44),
        ("{nf} shl", &[0x62, 0xF4, 0xF4, 0x1C, 0xD3, 0xE0][..], 0x45),
    ] {
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0x41, 0xFF, 0xC9]); // dec r9d (preserves CF)
        code.extend_from_slice(&[0x75, 0xF5]); // jnz to six-byte APX instruction
        code.push(0xF4);

        let mut expected = 4u64;
        for _ in 0..200 {
            expected = 1u64.wrapping_shl((expected & 0x3F) as u32);
        }
        let mut jit = make_vcpu_code(&code);
        let mut regs = jit.get_regs().unwrap();
        regs.rax = 1;
        regs.rcx = 4;
        regs.r9 = 200;
        regs.rflags = 0x3;
        jit.set_regs(&regs).unwrap();
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("jit APX NDD {name} CL alias: {error:?}")),
            "APX NDD {name} destination/CL alias loop must enter the native tier"
        );
        run_interp(&mut jit);
        let after = jit.get_regs().unwrap();
        assert_eq!(after.rax, 1, "{name}: source");
        assert_eq!(
            after.rcx, expected,
            "{name}: aliased count/result recurrence"
        );
        assert_eq!(after.r9 & 0xFFFF_FFFF, 0, "{name}: loop count");
        assert_eq!(
            after.rflags & STATUS_MASK,
            expected_status,
            "{name}: normal shift CF versus NF-preserved CF"
        );
    }
}

/// Variable shift by CL (`shl edx,cl`) in a hot loop — the pattern the kernel's
/// __free_pages_memory bootmem loop uses. Must JIT bit-exactly vs the interpreter.
#[test]
fn jit_shl_cl_matches_interpreter() {
    // mov ecx,5; xor eax,eax
    // loop: mov edx,1; shl edx,cl; add eax,edx; dec ecx; jns loop; hlt
    let code: &[u8] = &[
        0xB9, 0x05, 0x00, 0x00, 0x00, // mov ecx,5
        0x31, 0xC0, // xor eax,eax
        0xBA, 0x01, 0x00, 0x00, 0x00, // loop: mov edx,1
        0xD3, 0xE2, // shl edx,cl
        0x01, 0xD0, // add eax,edx
        0xFF, 0xC9, // dec ecx
        0x79, 0xF3, // jns loop
        0xF4, // hlt
    ];

    let mut jit = make_vcpu_code(code);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "SHL-by-CL loop should JIT and advance to its exit"
    );
    run_interp(&mut jit);
    let jr = jit.get_regs().unwrap();

    let mut interp = make_vcpu_code(code);
    run_interp(&mut interp);
    let ir = interp.get_regs().unwrap();

    assert_eq!(jr.rax, ir.rax, "rax (sum of 1<<cl for cl=5..0)");
    assert_eq!(jr.rdx, ir.rdx, "rdx (last shl result)");
    // Sum 32+16+8+4+2+1 = 63.
    assert_eq!(jr.rax & 0xffff_ffff, 63, "closed form sum");
}

/// CMOVcc in a hot loop (the conditional-move pattern the kernel bootmem loop
/// uses, `cmovge`). Must JIT bit-exactly vs the interpreter.
#[test]
fn jit_cmovge_matches_interpreter() {
    // mov ecx,100; xor eax,eax; mov ebx,0xFF
    // loop: add eax,1; cmp eax,50; cmovge ebx,eax; dec ecx; jnz loop; hlt
    let code: &[u8] = &[
        0xB9, 0x64, 0x00, 0x00, 0x00, // mov ecx,100
        0x31, 0xC0, // xor eax,eax
        0xBB, 0xFF, 0x00, 0x00, 0x00, // mov ebx,0xFF
        0x83, 0xC0, 0x01, // loop: add eax,1
        0x83, 0xF8, 0x32, // cmp eax,50
        0x0F, 0x4D, 0xD8, // cmovge ebx,eax
        0xFF, 0xC9, // dec ecx
        0x75, 0xF3, // jnz loop
        0xF4, // hlt
    ];

    let mut jit = make_vcpu_code(code);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "CMOVGE loop should JIT and advance to its exit"
    );
    run_interp(&mut jit);
    let jr = jit.get_regs().unwrap();

    let mut interp = make_vcpu_code(code);
    run_interp(&mut interp);
    let ir = interp.get_regs().unwrap();

    assert_eq!(jr.rax, ir.rax, "rax");
    assert_eq!(jr.rbx, ir.rbx, "rbx (cmovge target)");
    assert_eq!(jr.rcx, ir.rcx, "rcx");
    // eax 1..100; cmovge ebx,eax fires while eax>=50 → ebx ends at 100.
    assert_eq!(jr.rbx & 0xffff_ffff, 100, "closed-form cmovge result");
}

/// Loop with an EARLY forward exit to a separate continuation (two distinct
/// frontier exits + a back-edge) — the multi-exit CFG shape the kernel hot
/// regions use. Stresses the JIT's CFG / exit-PC lowering. Must match interp.
#[test]
fn jit_loop_early_exit_matches_interpreter() {
    // xor eax,eax; xor ebx,ebx; mov ecx,1000
    // loop: add eax,1; cmp eax,500; jge early; dec ecx; jnz loop; jmp late
    // early: mov ebx,0x1111; hlt
    // late:  mov ebx,0x2222; hlt
    let code: &[u8] = &[
        0x31, 0xC0, // xor eax,eax
        0x31, 0xDB, // xor ebx,ebx
        0xB9, 0xE8, 0x03, 0x00, 0x00, // mov ecx,1000
        0x83, 0xC0, 0x01, // loop: add eax,1
        0x3D, 0xF4, 0x01, 0x00, 0x00, // cmp eax,500
        0x7D, 0x06, // jge early
        0xFF, 0xC9, // dec ecx
        0x75, 0xF2, // jnz loop
        0xEB, 0x06, // jmp late
        0xBB, 0x11, 0x11, 0x00, 0x00, // early: mov ebx,0x1111
        0xF4, // hlt
        0xBB, 0x22, 0x22, 0x00, 0x00, // late: mov ebx,0x2222
        0xF4, // hlt
    ];

    let mut jit = make_vcpu_code(code);
    // May or may not promote in one shot; if it JITs, it must match the interp.
    let _ = jit.jit_try_block().expect("jit_try_block");
    run_interp(&mut jit);
    let jr = jit.get_regs().unwrap();

    let mut interp = make_vcpu_code(code);
    run_interp(&mut interp);
    let ir = interp.get_regs().unwrap();

    assert_eq!(jr.rax, ir.rax, "rax");
    assert_eq!(jr.rbx, ir.rbx, "rbx (which continuation ran)");
    assert_eq!(jr.rcx, ir.rcx, "rcx");
    // eax reaches 500 (iter 500) before ecx hits 0 → early taken → ebx=0x1111.
    assert_eq!(jr.rbx & 0xffff_ffff, 0x1111, "early continuation taken");
    assert_eq!(jr.rax & 0xffff_ffff, 500, "exited at eax==500");
}

/// Loop containing a CALL (a Call-terminator frontier the JIT exits through, as
/// the kernel hrtimer/text_poke hot regions do). The JIT runs up to the call,
/// hands back to the interpreter to execute call+ret, then re-enters. Final
/// state must match the pure interpreter.
#[test]
fn jit_loop_with_call_matches_interpreter() {
    // xor eax,eax; mov ecx,5
    // loop: add eax,1; call func; dec ecx; jnz loop; hlt
    // func: ret
    let code: &[u8] = &[
        0x31, 0xC0, // xor eax,eax
        0xB9, 0x05, 0x00, 0x00, 0x00, // mov ecx,5
        0x83, 0xC0, 0x01, // loop: add eax,1
        0xE8, 0x05, 0x00, 0x00, 0x00, // call func (rel32 +5)
        0xFF, 0xC9, // dec ecx
        0x75, 0xF4, // jnz loop
        0xF4, // hlt
        0xC3, // func: ret
    ];

    let mut jit = make_vcpu_code(code);
    let _ = jit.jit_try_block().expect("jit_try_block");
    run_interp(&mut jit);
    let jr = jit.get_regs().unwrap();

    let mut interp = make_vcpu_code(code);
    run_interp(&mut interp);
    let ir = interp.get_regs().unwrap();

    assert_eq!(jr.rax, ir.rax, "rax");
    assert_eq!(jr.rcx, ir.rcx, "rcx");
    assert_eq!(jr.rsp, ir.rsp, "rsp (call/ret balance)");
    assert_eq!(jr.rax & 0xffff_ffff, 5, "5 iterations");
}

/// Conditional CALL where the FALL-THROUGH (not the taken branch) is the
/// frontier — the exact polarity of the kernel hrtimer region (`test;jcc cont;
/// call`). Exercises the JIT exiting on a fall-through frontier with the correct
/// resume PC. Must match interp.
#[test]
fn jit_loop_cond_call_matches_interpreter() {
    // xor eax,eax; mov ecx,5
    // loop: add eax,1; test al,1; jnz cont; call func; cont: dec ecx; jnz loop; hlt
    // func: ret
    let code: &[u8] = &[
        0x31, 0xC0, // xor eax,eax
        0xB9, 0x05, 0x00, 0x00, 0x00, // mov ecx,5
        0x83, 0xC0, 0x01, // loop: add eax,1
        0xA8, 0x01, // test al,1
        0x75, 0x05, // jnz cont (skip call)
        0xE8, 0x05, 0x00, 0x00, 0x00, // call func (fall-through frontier)
        0xFF, 0xC9, // cont: dec ecx
        0x75, 0xF0, // jnz loop
        0xF4, // hlt
        0xC3, // func: ret
    ];

    let mut jit = make_vcpu_code(code);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "conditional CALL frontier loop should JIT and advance to its exit"
    );
    run_interp(&mut jit);
    let jr = jit.get_regs().unwrap();

    let mut interp = make_vcpu_code(code);
    run_interp(&mut interp);
    let ir = interp.get_regs().unwrap();

    assert_eq!(jr.rax, ir.rax, "rax");
    assert_eq!(jr.rcx, ir.rcx, "rcx");
    assert_eq!(jr.rsp, ir.rsp, "rsp (call/ret balance)");
    assert_eq!(jr.rax & 0xffff_ffff, 5, "5 iterations");
}

/// A realistic hot loop with an INTERNAL conditional (if-inside-loop): multiple
/// internal blocks, a forward branch + a join, two back-edges to the head, and a
/// HLT frontier — all run natively by `jit_try_block`. JIT final state must equal
/// the interpreter's (self-validating regardless of the exact arithmetic).
//   loop: add eax,1 ; cmp eax,10 ; jl skip ; add ebx,10 ; skip: dec ecx ; jnz loop ; hlt
#[test]
fn jit_loop_with_internal_if_matches_interp() {
    let code: Vec<u8> = vec![
        0x83, 0xC0, 0x01, // add eax,1
        0x83, 0xF8, 0x0A, // cmp eax,10
        0x7C, 0x03, // jl skip (+3 -> skip)   (eax<10 -> skip the add)
        0x83, 0xC3, 0x0A, // add ebx,10  (only when eax>=10)
        0xFF, 0xC9, // skip: dec ecx
        0x75, 0xF1, // jnz loop (-15 -> head)
        0xF4, // hlt
    ];
    let setup = |v: &mut X86_64Vcpu| {
        let mut r = v.get_regs().unwrap();
        r.rax = 0;
        r.rcx = 20;
        r.rbx = 0;
        v.set_regs(&r).unwrap();
    };

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "a loop with an internal if should JIT natively"
    );
    run_interp(&mut jit); // execute the parked HLT
    let jr = jit.get_regs().unwrap();

    let mut interp = make_vcpu_code(&code);
    setup(&mut interp);
    run_interp(&mut interp);
    let ir = interp.get_regs().unwrap();

    assert_eq!(jr.rax & 0xffff_ffff, ir.rax & 0xffff_ffff, "eax");
    assert_eq!(
        jr.rbx & 0xffff_ffff,
        ir.rbx & 0xffff_ffff,
        "ebx (conditional accumulation)"
    );
    assert_eq!(jr.rcx & 0xffff_ffff, ir.rcx & 0xffff_ffff, "ecx");
    assert_eq!(jr.rax & 0xffff_ffff, 20, "ran all 20 iterations");
    assert_eq!(
        jr.rbx & 0xffff_ffff,
        110,
        "ebx += 10 for each eax>=10 (iterations 10..=20)"
    );
}

/// General-exit lowering: a hot loop that exits to a NON-HLT continuation runs
/// natively (back-edge internal) and hands control back to the interpreter at
/// the loop-exit address via an exit stub recording `exit_pc`. The native
/// result + resume PC must match the interpreter stepped to the same point.
#[test]
fn jit_general_exit_matches_interp_at_handoff() {
    use rax::smir::ir::Terminator;
    use rax::smir::ir::memory::MemoryError;
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::x86_64::X86_64Lifter;
    use rax::smir::lift::{LiftContext, MemoryReader, SmirLifter};
    use rax::smir::lower::SmirLowerer;
    use rax::smir::lower::runtime::{ExecMem, GuestRegs, is_native_clobber_safe_excluding};
    use rax::smir::lower::x86_64::X86_64Lowerer;
    use std::collections::HashMap;

    // loop: add eax,2 ; dec ecx ; jnz loop      (exits to a continuation)
    // cont: mov ebx,0x7777 ; hlt
    let code: Vec<u8> = vec![
        0x83, 0xC0, 0x02, // add eax,2
        0xFF, 0xC9, // dec ecx
        0x75, 0xF9, // jnz loop (rel8 -7)
        0xBB, 0x77, 0x77, 0x00, 0x00, // mov ebx,0x7777  (continuation)
        0xF4, // hlt
    ];
    let cont_addr = LOAD_ADDR + 7;

    // --- JIT general-exit path ---
    struct Win {
        base: u64,
        bytes: Vec<u8>,
    }
    impl MemoryReader for Win {
        fn read(&self, addr: u64, size: usize) -> core::result::Result<Vec<u8>, MemoryError> {
            let off = addr
                .checked_sub(self.base)
                .filter(|&o| (o as usize) < self.bytes.len())
                .ok_or(MemoryError::OutOfBounds { addr })? as usize;
            let n = (self.bytes.len() - off).min(size);
            Ok(self.bytes[off..off + n].to_vec())
        }
    }
    let reader = Win {
        base: LOAD_ADDR,
        bytes: code.clone(),
    };
    let mut lifter = X86_64Lifter::strict();
    let mut lctx = LiftContext::new(SourceArch::X86_64);
    let func = lifter
        .lift_function(LOAD_ADDR, &reader, &mut lctx)
        .expect("lift_function");

    // Mark every "frontier" terminal (the JIT can't continue through it) as a
    // native-exit recording the block's guest_pc — the JIT runs up to but NOT
    // through it, so the interpreter resumes there and re-executes the block.
    let mut exits: HashMap<_, u64> = HashMap::new();
    for b in &func.blocks {
        let frontier = matches!(
            b.terminator,
            Terminator::Trap { .. }
                | Terminator::Return { .. }
                | Terminator::Call { .. }
                | Terminator::TailCall { .. }
                | Terminator::IndirectBranch { .. }
                | Terminator::IndirectBranchMem { .. }
                | Terminator::Switch { .. }
        );
        if frontier {
            exits.insert(b.id, b.guest_pc);
        }
    }
    assert!(!exits.is_empty(), "expected a frontier exit block");
    assert!(is_native_clobber_safe_excluding(&func, &exits, false));

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_native_exits(exits);
    let res = lowerer.lower_function(&func).expect("lower (native_exit)");
    assert!(res.relocations.is_empty());
    let bytes = lowerer.finalize().expect("finalize");
    let mem = ExecMem::new(&bytes).expect("ExecMem");

    let mut gr = GuestRegs::default();
    gr.gpr[0] = 0; // eax
    gr.gpr[1] = 10; // ecx (trip)
    gr.gpr[3] = 0x1111; // ebx (must NOT become 0x7777 — cont not executed)
    gr.rflags = 0x2;
    mem.run(res.entry_offset, &mut gr);

    // --- Interpreter, stepped to the same hand-off point ---
    let mut interp = make_vcpu_code(&code);
    let mut r = interp.get_regs().unwrap();
    r.rax = 0;
    r.rcx = 10;
    r.rbx = 0x1111;
    interp.set_regs(&r).unwrap();
    loop {
        if interp.get_regs().unwrap().rip == cont_addr {
            break;
        }
        match interp.step() {
            Ok(Some(VcpuExit::Hlt)) => panic!("interp hit HLT before the cont hand-off"),
            Ok(_) => {}
            Err(e) => panic!("interp error: {e:?}"),
        }
    }
    let ir = interp.get_regs().unwrap();

    assert_eq!(gr.exit_pc, cont_addr, "exit stub recorded the loop-exit PC");
    assert_eq!(gr.gpr[0] & 0xffff_ffff, ir.rax & 0xffff_ffff, "eax");
    assert_eq!(gr.gpr[1] & 0xffff_ffff, ir.rcx & 0xffff_ffff, "ecx");
    assert_eq!(
        gr.gpr[3] & 0xffff_ffff,
        ir.rbx & 0xffff_ffff,
        "ebx (continuation NOT run)"
    );
    assert_eq!(gr.gpr[0] & 0xffff_ffff, 20, "eax = 2*10");
    assert_eq!(
        gr.gpr[3] & 0xffff_ffff,
        0x1111,
        "ebx unchanged — exit block skipped"
    );
}

/// THE M5c GOAL: `run()` itself auto-detects the hot loop, compiles it, and
/// executes it natively — with no manual `jit_try_block` call — and produces the
/// correct architectural result. Proves the auto-trigger + cache + back-edge
/// hotness path end-to-end through the real run loop.
#[test]
fn run_auto_jits_hot_loop() {
    let n = 5000u32;
    let mut vcpu = make_vcpu(n);
    // run() services ~1ms slices and returns Hlt on the timer as well as on a
    // real guest HALT, so loop until the guest loop has actually drained
    // (ecx==0), bounded so a bug can't hang the test.
    let mut slices = 0;
    loop {
        let _ = vcpu.run().expect("run");
        slices += 1;
        if vcpu.get_regs().unwrap().rcx & 0xffff_ffff == 0 || slices > 10_000 {
            break;
        }
    }
    let r = vcpu.get_regs().unwrap();
    assert_eq!(r.rcx & 0xffff_ffff, 0, "the hot loop drained under run()");
    assert_eq!(
        r.rax & 0xffff_ffff,
        (2 * n as u64) & 0xffff_ffff,
        "eax = 2*n (correct result)"
    );
    // The auto-trigger must have fired: at least one region was compiled.
    assert!(
        vcpu.jit_region_count() >= 1,
        "run() should have auto-compiled the hot loop, got {} regions",
        vcpu.jit_region_count()
    );
}

/// SMC safety: a guest store to a code page that has a cached JIT region must
/// EVICT that region (so stale native code never runs) — essential for a kernel
/// that patches/loads code. Control case confirms the region would otherwise
/// stay cached.
#[test]
fn run_smc_evicts_jit_region() {
    // add eax,1 ; dec ecx ; jnz loop   (back-edge to LOAD_ADDR)
    let loop_bytes = [0x83, 0xC0, 0x01, 0xFF, 0xC9, 0x75, 0xF9];
    let mk = |store: bool| -> X86_64Vcpu {
        let mut code = loop_bytes.to_vec();
        if store {
            // mov rbx, LOAD_ADDR ; mov byte [rbx], 0x90   (self-modify code page)
            code.extend_from_slice(&[0x48, 0xBB]);
            code.extend_from_slice(&LOAD_ADDR.to_le_bytes());
            code.extend_from_slice(&[0xC6, 0x03, 0x90]);
        }
        code.push(0xF4); // hlt
        let mut v = make_vcpu_code(&code);
        let mut r = v.get_regs().unwrap();
        r.rcx = 500; // well past the 64-hit JIT threshold
        v.set_regs(&r).unwrap();
        v
    };
    let drive = |v: &mut X86_64Vcpu| {
        for _ in 0..10_000 {
            let _ = v.run().expect("run");
            if v.get_regs().unwrap().rcx & 0xffff_ffff == 0 {
                let _ = v.run().expect("run"); // let the continuation (store+hlt) finish
                break;
            }
        }
    };

    // Control: no self-modifying write — the compiled region remains cached.
    let mut a = mk(false);
    drive(&mut a);
    assert!(
        a.jit_region_count() >= 1,
        "control: the hot loop should compile and stay cached (got {})",
        a.jit_region_count()
    );

    // SMC: the guest store to the loop's code page must evict the region.
    let mut b = mk(true);
    drive(&mut b);
    assert_eq!(
        b.jit_region_count(),
        0,
        "a self-modifying store must evict the cached JIT region"
    );
}

/// Report JIT vs interpreter throughput on the same loop (informational).
#[test]
fn jit_throughput() {
    // Large count: the whole loop runs in ONE native call (internal back-edge).
    let big = 200_000_000u32;
    let mut jit = make_vcpu(big);
    let t = Instant::now();
    let ran = jit.jit_try_block().expect("jit_try_block");
    let dt = t.elapsed();
    assert!(ran, "the loop region should JIT");
    let executed = (big as u64) * 5 + 3; // matches bench_loop accounting
    let mips = executed as f64 / dt.as_secs_f64() / 1e6;
    let r = jit.get_regs().unwrap();
    println!(
        "[jit-vcpu] {} insns in {:.4}s => {:.1} MIPS  (final eax={:#x} ecx={:#x})",
        executed,
        dt.as_secs_f64(),
        mips,
        r.rax & 0xffff_ffff,
        r.rcx & 0xffff_ffff
    );
    assert_eq!(r.rax & 0xffff_ffff, (2 * big as u64) & 0xffff_ffff);
}

/// Build a vcpu loaded with `code`, returning the guest memory so a test can
/// seed/inspect scratch data. Same long-mode flat config as `make_vcpu_code`.
fn make_vcpu_mem(code: &[u8]) -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
    let region = MmapRegion::new(MEM_SIZE as usize).unwrap();
    let guest_region = GuestRegionMmap::new(region, GuestAddress(0)).unwrap();
    let memory = Arc::new(GuestMemoryMmap::from_regions(vec![guest_region]).unwrap());
    memory.write_slice(code, GuestAddress(LOAD_ADDR)).unwrap();

    let mut regs = Registers::default();
    regs.rip = LOAD_ADDR;
    regs.rsp = 0x11_0000;
    regs.rflags = 0x2;
    let mut sregs = SystemRegisters::default();
    sregs.cr0 = 0x21;
    sregs.cr4 = 0x20;
    sregs.efer = 0x500;
    sregs.cs.base = 0;
    sregs.cs.limit = 0xFFFFFFFF;
    sregs.cs.selector = 0x8;
    sregs.cs.type_ = 0xB;
    sregs.cs.present = true;
    sregs.cs.s = true;
    sregs.cs.l = true;
    sregs.cs.g = true;
    sregs.ds.base = 0;
    sregs.ds.limit = 0xFFFFFFFF;
    sregs.ds.selector = 0x10;
    sregs.ds.type_ = 0x3;
    sregs.ds.present = true;
    sregs.ds.db = true;
    sregs.ds.s = true;
    sregs.ds.g = true;
    sregs.es = sregs.ds.clone();
    sregs.fs = sregs.ds.clone();
    sregs.gs = sregs.ds.clone();
    sregs.ss = sregs.ds.clone();

    let mut vcpu = X86_64Vcpu::new(0, memory.clone());
    vcpu.set_regs(&regs).unwrap();
    vcpu.set_sregs(&sregs).unwrap();
    (vcpu, memory)
}

#[test]
fn jit_vector_register_moves_match_legacy_preservation_and_vex_zeroing() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    for (name, code, preserves_upper) in [
        (
            "MOVAPS xmm1,xmm0",
            &[0x0F, 0x28, 0xC8, 0xFF, 0xC9, 0x75, 0xF9, 0xF4][..],
            true,
        ),
        (
            "VMOVAPS xmm1,xmm0",
            &[0xC5, 0xF8, 0x28, 0xC8, 0xFF, 0xC9, 0x75, 0xF8, 0xF4][..],
            false,
        ),
    ] {
        let setup = |vcpu: &mut X86_64Vcpu| {
            let mut regs = vcpu.get_regs().unwrap();
            regs.rcx = 1;
            regs.xmm[0] = [0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210];
            regs.ymm_high[0] = [0x1010_2020_3030_4040, 0x5050_6060_7070_8080];
            regs.zmm_high[0] = [0x1111, 0x2222, 0x3333, 0x4444];
            regs.xmm[1] = [0xAAAA_AAAA_AAAA_AAAA, 0xBBBB_BBBB_BBBB_BBBB];
            regs.ymm_high[1] = [0xCCCC_CCCC_CCCC_CCCC, 0xDDDD_DDDD_DDDD_DDDD];
            regs.zmm_high[1] = [0xEEEE, 0xFFFF, 0xABCD, 0xDCBA];
            vcpu.set_regs(&regs).unwrap();
            regs
        };

        let (mut interp, _) = make_vcpu_mem(code);
        let initial = setup(&mut interp);
        run_interp(&mut interp);
        let interp_regs = interp.get_regs().unwrap();

        let (mut jit, _) = make_vcpu_mem(code);
        setup(&mut jit);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error:?}")),
            "{name}: architectural register move should JIT"
        );
        run_interp(&mut jit);
        let jit_regs = jit.get_regs().unwrap();

        assert_eq!(jit_regs.xmm, interp_regs.xmm, "{name}: low XMM state");
        assert_eq!(
            jit_regs.ymm_high, interp_regs.ymm_high,
            "{name}: YMM upper state"
        );
        assert_eq!(
            jit_regs.zmm_high, interp_regs.zmm_high,
            "{name}: ZMM upper state"
        );
        assert_eq!(jit_regs.xmm[1], initial.xmm[0], "{name}: low transfer");
        if preserves_upper {
            assert_eq!(jit_regs.ymm_high[1], initial.ymm_high[1], "{name}");
            assert_eq!(jit_regs.zmm_high[1], initial.zmm_high[1], "{name}");
        } else {
            assert_eq!(jit_regs.ymm_high[1], [0; 2], "{name}");
            assert_eq!(jit_regs.zmm_high[1], [0; 4], "{name}");
        }
    }
}

#[test]
fn jit_vector_logic_matches_legacy_and_vex_evex_lane_semantics() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx2")
    {
        return;
    }

    // loop: andps xmm1,xmm2; vxorps xmm4,xmm5,xmm6;
    //       vpandn ymm7,ymm8,ymm9; dec ecx; jnz loop; hlt
    let code = [
        0x0F, 0x54, 0xCA, 0xC5, 0xD0, 0x57, 0xE6, 0xC4, 0xC1, 0x3D, 0xDF, 0xF9, 0xFF, 0xC9, 0x75,
        0xF0, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;

        regs.xmm[1] = [0xFFFF_0000_F0F0_0F0F, 0x1234_5678_9ABC_DEF0];
        regs.xmm[2] = [0x0FF0_FF00_3333_CCCC, 0xFFFF_0000_FFFF_0000];
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [0x9999, 0xAAAA, 0xBBBB, 0xCCCC];

        regs.xmm[5] = [0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210];
        regs.xmm[6] = [0xFFFF_0000_AAAA_5555, 0x1357_9BDF_2468_ACE0];
        regs.ymm_high[4] = [0xDEAD_BEEF_DEAD_BEEF, 0xCAFE_BABE_CAFE_BABE];
        regs.zmm_high[4] = [1, 2, 3, 4];

        regs.xmm[8] = [0xFFFF_0000_FFFF_0000, 0xAAAA_AAAA_5555_5555];
        regs.ymm_high[8] = [0x0123_4567_89AB_CDEF, 0x0F0F_F0F0_3333_CCCC];
        regs.xmm[9] = [0x1234_5678_9ABC_DEF0, 0xFFFF_FFFF_0000_0000];
        regs.ymm_high[9] = [0xFFFF_0000_AAAA_5555, 0xF0F0_0F0F_FFFF_0000];
        regs.zmm_high[7] = [5, 6, 7, 8];
        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();

    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(jit.jit_try_block().expect("vector logic JIT eligibility"));
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(
        jit_regs.xmm[1],
        [
            initial.xmm[1][0] & initial.xmm[2][0],
            initial.xmm[1][1] & initial.xmm[2][1],
        ],
        "legacy ANDPS result"
    );
    assert_eq!(jit_regs.ymm_high[1], initial.ymm_high[1]);
    assert_eq!(jit_regs.zmm_high[1], initial.zmm_high[1]);
    assert_eq!(
        jit_regs.xmm[4],
        [
            initial.xmm[5][0] ^ initial.xmm[6][0],
            initial.xmm[5][1] ^ initial.xmm[6][1],
        ],
        "VEX VXORPS result"
    );
    assert_eq!(jit_regs.ymm_high[4], [0; 2]);
    assert_eq!(jit_regs.zmm_high[4], [0; 4]);
    assert_eq!(
        jit_regs.xmm[7],
        [
            !initial.xmm[8][0] & initial.xmm[9][0],
            !initial.xmm[8][1] & initial.xmm[9][1],
        ],
        "VEX VPANDN low lane"
    );
    assert_eq!(
        jit_regs.ymm_high[7],
        [
            !initial.ymm_high[8][0] & initial.ymm_high[9][0],
            !initial.ymm_high[8][1] & initial.ymm_high[9][1],
        ],
        "VEX VPANDN high lane"
    );
    assert_eq!(jit_regs.zmm_high[7], [0; 4]);

    if !std::is_x86_feature_detected!("avx512dq") {
        return;
    }

    // loop: vandpd zmm4,zmm5,zmm6; dec ecx; jnz loop; hlt
    let evex_code = [
        0x62, 0xF1, 0xD5, 0x48, 0x54, 0xE6, 0xFF, 0xC9, 0x75, 0xF6, 0xF4,
    ];
    let setup_evex = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.xmm[5] = [0xFFFF_0000_F0F0_0F0F, 0x1234_5678_9ABC_DEF0];
        regs.ymm_high[5] = [0xAAAA_5555_AAAA_5555, 0x0F0F_F0F0_3333_CCCC];
        regs.zmm_high[5] = [1, 2, 3, 4];
        regs.xmm[6] = [0x0FF0_FF00_3333_CCCC, 0xFFFF_0000_FFFF_0000];
        regs.ymm_high[6] = [0xFFFF_0000_AAAA_5555, 0xF0F0_0F0F_FFFF_0000];
        regs.zmm_high[6] = [5, 6, 7, 8];
        vcpu.set_regs(&regs).unwrap();
        regs
    };
    let (mut interp, _) = make_vcpu_mem(&evex_code);
    let initial = setup_evex(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&evex_code);
    setup_evex(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("EVEX vector logic JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();
    assert_eq!(jit_regs.xmm[4], interp_regs.xmm[4]);
    assert_eq!(jit_regs.ymm_high[4], interp_regs.ymm_high[4]);
    assert_eq!(jit_regs.zmm_high[4], interp_regs.zmm_high[4]);
    assert_eq!(
        jit_regs.xmm[4],
        [
            initial.xmm[5][0] & initial.xmm[6][0],
            initial.xmm[5][1] & initial.xmm[6][1],
        ],
        "EVEX VANDPD low 128 bits"
    );
    assert_eq!(
        jit_regs.ymm_high[4],
        [
            initial.ymm_high[5][0] & initial.ymm_high[6][0],
            initial.ymm_high[5][1] & initial.ymm_high[6][1],
        ],
        "EVEX VANDPD bits 255:128"
    );
    assert_eq!(
        jit_regs.zmm_high[4],
        [
            initial.zmm_high[5][0] & initial.zmm_high[6][0],
            initial.zmm_high[5][1] & initial.zmm_high[6][1],
            initial.zmm_high[5][2] & initial.zmm_high[6][2],
            initial.zmm_high[5][3] & initial.zmm_high[6][3],
        ],
        "EVEX VANDPD bits 511:256"
    );
}

#[test]
fn jit_packed_integer_add_sub_matches_lane_wrapping_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx2")
    {
        return;
    }

    // loop: paddb xmm1,xmm2; vpaddd xmm4,xmm5,xmm6;
    //       vpsubw ymm7,ymm8,ymm9; dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0xFC, 0xCA, 0xC5, 0xD1, 0xFE, 0xE6, 0xC4, 0xC1, 0x3D, 0xF9, 0xF9, 0xFF, 0xC9,
        0x75, 0xEF, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.xmm[1] = [0xFF01_7F80_10F0_00FE, 0xABCD_EF01_2345_6789];
        regs.xmm[2] = [0x0203_0180_F020_FF05, 0x9977_5533_11FF_DDBB];
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [0x9999, 0xAAAA, 0xBBBB, 0xCCCC];

        regs.xmm[5] = [0xFFFF_FFFF_7FFF_FFFF, 0x8000_0000_0123_4567];
        regs.xmm[6] = [0x0000_0001_0000_0002, 0x8000_0000_FEDC_BA99];
        regs.ymm_high[4] = [0xDEAD_BEEF_DEAD_BEEF, 0xCAFE_BABE_CAFE_BABE];
        regs.zmm_high[4] = [1, 2, 3, 4];

        regs.xmm[8] = [0x0000_FFFF_8000_7FFF, 0xAAAA_5555_0001_FFFE];
        regs.ymm_high[8] = [0x1234_5678_9ABC_DEF0, 0xFFFF_0000_8000_7FFF];
        regs.xmm[9] = [0x0001_0002_7FFF_8000, 0x5555_AAAA_FFFF_0002];
        regs.ymm_high[9] = [0x4321_8765_CBA9_0FED, 0x0001_FFFF_7FFF_8000];
        regs.zmm_high[7] = [5, 6, 7, 8];
        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let add_bytes = |first: u64, second: u64| {
        let mut result = 0u64;
        for byte in 0..8 {
            let shift = byte * 8;
            let value = ((first >> shift) as u8).wrapping_add((second >> shift) as u8);
            result |= u64::from(value) << shift;
        }
        result
    };
    let add_dwords = |first: u64, second: u64| {
        u64::from((first as u32).wrapping_add(second as u32))
            | (u64::from(((first >> 32) as u32).wrapping_add((second >> 32) as u32)) << 32)
    };
    let sub_words = |first: u64, second: u64| {
        let mut result = 0u64;
        for word in 0..4 {
            let shift = word * 16;
            let value = ((first >> shift) as u16).wrapping_sub((second >> shift) as u16);
            result |= u64::from(value) << shift;
        }
        result
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("packed integer add/sub JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(
        jit_regs.xmm[1],
        [
            add_bytes(initial.xmm[1][0], initial.xmm[2][0]),
            add_bytes(initial.xmm[1][1], initial.xmm[2][1]),
        ],
        "legacy PADDB wrapping"
    );
    assert_eq!(jit_regs.ymm_high[1], initial.ymm_high[1]);
    assert_eq!(jit_regs.zmm_high[1], initial.zmm_high[1]);
    assert_eq!(
        jit_regs.xmm[4],
        [
            add_dwords(initial.xmm[5][0], initial.xmm[6][0]),
            add_dwords(initial.xmm[5][1], initial.xmm[6][1]),
        ],
        "VEX VPADDD wrapping"
    );
    assert_eq!(jit_regs.ymm_high[4], [0; 2]);
    assert_eq!(jit_regs.zmm_high[4], [0; 4]);
    assert_eq!(
        jit_regs.xmm[7],
        [
            sub_words(initial.xmm[8][0], initial.xmm[9][0]),
            sub_words(initial.xmm[8][1], initial.xmm[9][1]),
        ],
        "VEX VPSUBW low lane"
    );
    assert_eq!(
        jit_regs.ymm_high[7],
        [
            sub_words(initial.ymm_high[8][0], initial.ymm_high[9][0]),
            sub_words(initial.ymm_high[8][1], initial.ymm_high[9][1]),
        ],
        "VEX VPSUBW high lane"
    );
    assert_eq!(jit_regs.zmm_high[7], [0; 4]);

    // loop: vpaddq zmm4,zmm5,zmm6; dec ecx; jnz loop; hlt
    let evex_code = [
        0x62, 0xF1, 0xD5, 0x48, 0xD4, 0xE6, 0xFF, 0xC9, 0x75, 0xF6, 0xF4,
    ];
    let setup_evex = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.xmm[5] = [u64::MAX, 0x7FFF_FFFF_FFFF_FFFF];
        regs.ymm_high[5] = [0x8000_0000_0000_0000, 0x0123_4567_89AB_CDEF];
        regs.zmm_high[5] = [1, 2, u64::MAX, 0xAAAA_AAAA_AAAA_AAAA];
        regs.xmm[6] = [2, 1];
        regs.ymm_high[6] = [0x8000_0000_0000_0000, 0xFEDC_BA98_7654_3211];
        regs.zmm_high[6] = [3, u64::MAX, 7, 0x5555_5555_5555_5556];
        vcpu.set_regs(&regs).unwrap();
        regs
    };
    let (mut interp, _) = make_vcpu_mem(&evex_code);
    let initial = setup_evex(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&evex_code);
    setup_evex(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("EVEX packed integer add JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();
    assert_eq!(jit_regs.xmm[4], interp_regs.xmm[4]);
    assert_eq!(jit_regs.ymm_high[4], interp_regs.ymm_high[4]);
    assert_eq!(jit_regs.zmm_high[4], interp_regs.zmm_high[4]);
    assert_eq!(
        jit_regs.xmm[4],
        [
            initial.xmm[5][0].wrapping_add(initial.xmm[6][0]),
            initial.xmm[5][1].wrapping_add(initial.xmm[6][1]),
        ]
    );
    assert_eq!(
        jit_regs.ymm_high[4],
        [
            initial.ymm_high[5][0].wrapping_add(initial.ymm_high[6][0]),
            initial.ymm_high[5][1].wrapping_add(initial.ymm_high[6][1]),
        ]
    );
    assert_eq!(
        jit_regs.zmm_high[4],
        [
            initial.zmm_high[5][0].wrapping_add(initial.zmm_high[6][0]),
            initial.zmm_high[5][1].wrapping_add(initial.zmm_high[6][1]),
            initial.zmm_high[5][2].wrapping_add(initial.zmm_high[6][2]),
            initial.zmm_high[5][3].wrapping_add(initial.zmm_high[6][3]),
        ]
    );
}

#[test]
fn jit_saturating_integer_add_sub_matches_lane_clamps_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx2")
    {
        return;
    }

    // loop: paddsb xmm1,xmm2; psubusw xmm3,xmm4;
    //       vpaddusb xmm5,xmm6,xmm7; vpsubsw ymm8,ymm9,ymm10;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0xEC, 0xCA, 0x66, 0x0F, 0xD9, 0xDC, 0xC5, 0xC9, 0xDC, 0xEF, 0xC4, 0x41, 0x35,
        0xE9, 0xC2, 0xFF, 0xC9, 0x75, 0xEB, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;

        regs.xmm[1] = [0x8878_817F_80F0_1078, 0x7F80_7F80_7F80_7F80];
        regs.xmm[2] = [0xF614_FF01_FF20_7878, 0x017F_FF80_4010_C080];
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [1, 2, 3, 4];

        regs.xmm[3] = [0x0001_0000_FFFF_8000, 0x1234_5678_9ABC_DEF0];
        regs.xmm[4] = [0x0002_0001_0001_7FFF, 0x2345_1111_ABCD_EF01];
        regs.ymm_high[3] = [0x9999_AAAA_BBBB_CCCC, 0xDDDD_EEEE_FFFF_0000];
        regs.zmm_high[3] = [5, 6, 7, 8];

        regs.xmm[6] = [0xFFFE_FDFC_807F_0100, 0xF0E0_D0C0_B0A0_9080];
        regs.xmm[7] = [0x0203_0405_FF02_01FF, 0x2030_4050_6070_8090];
        regs.ymm_high[5] = [0xDEAD_BEEF_DEAD_BEEF, 0xCAFE_BABE_CAFE_BABE];
        regs.zmm_high[5] = [9, 10, 11, 12];

        regs.xmm[9] = [0x7FFF_8000_7000_9000, 0x0001_FFFF_4000_C000];
        regs.xmm[10] = [0xFFFF_0001_F000_1000, 0x8000_7FFF_C000_4000];
        regs.ymm_high[9] = [0x7FFF_8000_1234_EDCC, 0x6000_A000_0000_FFFF];
        regs.ymm_high[10] = [0xFFFF_0001_8000_7FFF, 0xE000_2000_0001_8000];
        regs.zmm_high[8] = [13, 14, 15, 16];
        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let add_signed_bytes = |first: u64, second: u64| {
        let mut result = 0u64;
        for byte in 0..8 {
            let shift = byte * 8;
            let lhs = (first >> shift) as u8 as i8;
            let rhs = (second >> shift) as u8 as i8;
            result |= u64::from(lhs.saturating_add(rhs) as u8) << shift;
        }
        result
    };
    let add_unsigned_bytes = |first: u64, second: u64| {
        let mut result = 0u64;
        for byte in 0..8 {
            let shift = byte * 8;
            let lhs = (first >> shift) as u8;
            let rhs = (second >> shift) as u8;
            result |= u64::from(lhs.saturating_add(rhs)) << shift;
        }
        result
    };
    let sub_unsigned_bytes = |first: u64, second: u64| {
        let mut result = 0u64;
        for byte in 0..8 {
            let shift = byte * 8;
            let lhs = (first >> shift) as u8;
            let rhs = (second >> shift) as u8;
            result |= u64::from(lhs.saturating_sub(rhs)) << shift;
        }
        result
    };
    let add_signed_words = |first: u64, second: u64| {
        let mut result = 0u64;
        for word in 0..4 {
            let shift = word * 16;
            let lhs = (first >> shift) as u16 as i16;
            let rhs = (second >> shift) as u16 as i16;
            result |= u64::from(lhs.saturating_add(rhs) as u16) << shift;
        }
        result
    };
    let sub_signed_words = |first: u64, second: u64| {
        let mut result = 0u64;
        for word in 0..4 {
            let shift = word * 16;
            let lhs = (first >> shift) as u16 as i16;
            let rhs = (second >> shift) as u16 as i16;
            result |= u64::from(lhs.saturating_sub(rhs) as u16) << shift;
        }
        result
    };
    let sub_unsigned_words = |first: u64, second: u64| {
        let mut result = 0u64;
        for word in 0..4 {
            let shift = word * 16;
            let lhs = (first >> shift) as u16;
            let rhs = (second >> shift) as u16;
            result |= u64::from(lhs.saturating_sub(rhs)) << shift;
        }
        result
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("saturating integer add/sub JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(
        jit_regs.xmm[1],
        [
            add_signed_bytes(initial.xmm[1][0], initial.xmm[2][0]),
            add_signed_bytes(initial.xmm[1][1], initial.xmm[2][1]),
        ],
        "legacy PADDSB clamps"
    );
    assert_eq!(jit_regs.ymm_high[1], initial.ymm_high[1]);
    assert_eq!(jit_regs.zmm_high[1], initial.zmm_high[1]);
    assert_eq!(
        jit_regs.xmm[3],
        [
            sub_unsigned_words(initial.xmm[3][0], initial.xmm[4][0]),
            sub_unsigned_words(initial.xmm[3][1], initial.xmm[4][1]),
        ],
        "legacy PSUBUSW clamps"
    );
    assert_eq!(jit_regs.ymm_high[3], initial.ymm_high[3]);
    assert_eq!(jit_regs.zmm_high[3], initial.zmm_high[3]);
    assert_eq!(
        jit_regs.xmm[5],
        [
            add_unsigned_bytes(initial.xmm[6][0], initial.xmm[7][0]),
            add_unsigned_bytes(initial.xmm[6][1], initial.xmm[7][1]),
        ],
        "VEX VPADDUSB clamps"
    );
    assert_eq!(jit_regs.ymm_high[5], [0; 2]);
    assert_eq!(jit_regs.zmm_high[5], [0; 4]);
    assert_eq!(
        jit_regs.xmm[8],
        [
            sub_signed_words(initial.xmm[9][0], initial.xmm[10][0]),
            sub_signed_words(initial.xmm[9][1], initial.xmm[10][1]),
        ],
        "VEX VPSUBSW low lane clamps"
    );
    assert_eq!(
        jit_regs.ymm_high[8],
        [
            sub_signed_words(initial.ymm_high[9][0], initial.ymm_high[10][0]),
            sub_signed_words(initial.ymm_high[9][1], initial.ymm_high[10][1]),
        ],
        "VEX VPSUBSW high lane clamps"
    );
    assert_eq!(jit_regs.zmm_high[8], [0; 4]);

    // loop: vpaddsw zmm4,zmm5,zmm6; vpsubusb zmm7,zmm8,zmm9;
    //       dec ecx; jnz loop; hlt
    let evex_code = [
        0x62, 0xF1, 0x55, 0x48, 0xED, 0xE6, 0x62, 0xD1, 0x3D, 0x48, 0xD8, 0xF9, 0xFF, 0xC9, 0x75,
        0xF0, 0xF4,
    ];
    let setup_evex = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.xmm[5] = [0x7FFF_8000_7000_9000, 0x1111_EEEE_4000_C000];
        regs.ymm_high[5] = [0x7FFF_8000_0001_FFFF, 0x6000_A000_1234_EDCC];
        regs.zmm_high[5] = [0x4000_C000_7FFE_8001, 1, u64::MAX, 0x5555_AAAA_0000_FFFF];
        regs.xmm[6] = [0x0001_FFFF_1000_F000, 0x7000_9000_4000_C000];
        regs.ymm_high[6] = [0x7FFF_8000_FFFF_0001, 0x3000_D000_EDCC_1234];
        regs.zmm_high[6] = [0x4000_C000_0002_FFFE, u64::MAX, 1, 0x5555_AAAA_FFFF_0001];

        regs.xmm[8] = [0x0001_0203_FFFE_FDFC, 0x1020_3040_5060_7080];
        regs.ymm_high[8] = [0xFF00_807F_0102_0304, 0xA0B0_C0D0_E0F0_FFFF];
        regs.zmm_high[8] = [0, 1, u64::MAX, 0x5555_AAAA_00FF_FF00];
        regs.xmm[9] = [0x0102_0104_0203_FFFF, 0x2010_4030_6050_8070];
        regs.ymm_high[9] = [0x01FF_7F80_0201_0403, 0xB0A0_D0C0_F0E0_01FF];
        regs.zmm_high[9] = [1, u64::MAX, 1, 0xAAAA_5555_FF00_00FF];
        vcpu.set_regs(&regs).unwrap();
        regs
    };
    let (mut interp, _) = make_vcpu_mem(&evex_code);
    let initial = setup_evex(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&evex_code);
    setup_evex(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("EVEX saturating integer add/sub JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "EVEX low XMM state");
    assert_eq!(
        jit_regs.ymm_high, interp_regs.ymm_high,
        "EVEX YMM upper state"
    );
    assert_eq!(
        jit_regs.zmm_high, interp_regs.zmm_high,
        "EVEX ZMM upper state"
    );
    for (result, first, second, operation) in [
        (
            4usize,
            5usize,
            6usize,
            add_signed_words as fn(u64, u64) -> u64,
        ),
        (
            7usize,
            8usize,
            9usize,
            sub_unsigned_bytes as fn(u64, u64) -> u64,
        ),
    ] {
        assert_eq!(
            jit_regs.xmm[result],
            [
                operation(initial.xmm[first][0], initial.xmm[second][0]),
                operation(initial.xmm[first][1], initial.xmm[second][1]),
            ]
        );
        assert_eq!(
            jit_regs.ymm_high[result],
            [
                operation(initial.ymm_high[first][0], initial.ymm_high[second][0],),
                operation(initial.ymm_high[first][1], initial.ymm_high[second][1],),
            ]
        );
        assert_eq!(
            jit_regs.zmm_high[result],
            [
                operation(initial.zmm_high[first][0], initial.zmm_high[second][0],),
                operation(initial.zmm_high[first][1], initial.zmm_high[second][1],),
                operation(initial.zmm_high[first][2], initial.zmm_high[second][2],),
                operation(initial.zmm_high[first][3], initial.zmm_high[second][3],),
            ]
        );
    }
}

#[test]
fn jit_low_packed_integer_multiply_matches_lane_wrapping_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512dq")
        || !std::is_x86_feature_detected!("avx2")
        || !std::is_x86_feature_detected!("sse4.1")
    {
        return;
    }

    // loop: pmullw xmm1,xmm2; pmulld xmm3,xmm4;
    //       vpmullw xmm5,xmm6,xmm7; vpmulld ymm8,ymm9,ymm10;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0xD5, 0xCA, 0x66, 0x0F, 0x38, 0x40, 0xDC, 0xC5, 0xC9, 0xD5, 0xEF, 0xC4, 0x42,
        0x35, 0x40, 0xC2, 0xFF, 0xC9, 0x75, 0xEA, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;

        regs.xmm[1] = [0xFFFF_8000_7FFF_1234, 0xABCD_0002_FFFE_4000];
        regs.xmm[2] = [0x0002_FFFF_0003_1000, 0x1000_FFFF_0003_0004];
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [1, 2, 3, 4];

        regs.xmm[3] = [0xFFFF_FFFF_8000_0000, 0x7FFF_FFFF_1234_5678];
        regs.xmm[4] = [0x0000_0002_FFFF_FFFF, 0x0000_0003_1000_0000];
        regs.ymm_high[3] = [0x9999_AAAA_BBBB_CCCC, 0xDDDD_EEEE_FFFF_0000];
        regs.zmm_high[3] = [5, 6, 7, 8];

        regs.xmm[6] = [0x8000_7FFF_FFFF_0002, 0x1234_5678_9ABC_DEF0];
        regs.xmm[7] = [0xFFFF_0003_0002_8000, 0xFEDC_BA98_7654_3210];
        regs.ymm_high[5] = [0xDEAD_BEEF_DEAD_BEEF, 0xCAFE_BABE_CAFE_BABE];
        regs.zmm_high[5] = [9, 10, 11, 12];

        regs.xmm[9] = [0xFFFF_FFFF_8000_0000, 0x7FFF_FFFF_0123_4567];
        regs.xmm[10] = [0x0000_0002_FFFF_FFFF, 0x0000_0003_FEDC_BA99];
        regs.ymm_high[9] = [0x8000_0000_0000_0002, 0xFFFF_FFFF_7FFF_FFFF];
        regs.ymm_high[10] = [0xFFFF_FFFF_8000_0000, 0x0000_0002_0000_0003];
        regs.zmm_high[8] = [13, 14, 15, 16];
        vcpu.set_regs(&regs).unwrap();
        regs
    };
    let mul_words = |first: u64, second: u64| {
        let mut result = 0u64;
        for word in 0..4 {
            let shift = word * 16;
            let lhs = (first >> shift) as u16;
            let rhs = (second >> shift) as u16;
            result |= u64::from(lhs.wrapping_mul(rhs)) << shift;
        }
        result
    };
    let mul_dwords = |first: u64, second: u64| {
        u64::from((first as u32).wrapping_mul(second as u32))
            | (u64::from(((first >> 32) as u32).wrapping_mul((second >> 32) as u32)) << 32)
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("low packed integer multiply JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(
        jit_regs.xmm[1],
        [
            mul_words(initial.xmm[1][0], initial.xmm[2][0]),
            mul_words(initial.xmm[1][1], initial.xmm[2][1]),
        ],
        "legacy PMULLW wrapping"
    );
    assert_eq!(jit_regs.ymm_high[1], initial.ymm_high[1]);
    assert_eq!(jit_regs.zmm_high[1], initial.zmm_high[1]);
    assert_eq!(
        jit_regs.xmm[3],
        [
            mul_dwords(initial.xmm[3][0], initial.xmm[4][0]),
            mul_dwords(initial.xmm[3][1], initial.xmm[4][1]),
        ],
        "legacy PMULLD wrapping"
    );
    assert_eq!(jit_regs.ymm_high[3], initial.ymm_high[3]);
    assert_eq!(jit_regs.zmm_high[3], initial.zmm_high[3]);
    assert_eq!(
        jit_regs.xmm[5],
        [
            mul_words(initial.xmm[6][0], initial.xmm[7][0]),
            mul_words(initial.xmm[6][1], initial.xmm[7][1]),
        ],
        "VEX VPMULLW wrapping"
    );
    assert_eq!(jit_regs.ymm_high[5], [0; 2]);
    assert_eq!(jit_regs.zmm_high[5], [0; 4]);
    assert_eq!(
        jit_regs.xmm[8],
        [
            mul_dwords(initial.xmm[9][0], initial.xmm[10][0]),
            mul_dwords(initial.xmm[9][1], initial.xmm[10][1]),
        ],
        "VEX VPMULLD low lane wrapping"
    );
    assert_eq!(
        jit_regs.ymm_high[8],
        [
            mul_dwords(initial.ymm_high[9][0], initial.ymm_high[10][0]),
            mul_dwords(initial.ymm_high[9][1], initial.ymm_high[10][1]),
        ],
        "VEX VPMULLD high lane wrapping"
    );
    assert_eq!(jit_regs.zmm_high[8], [0; 4]);

    // loop: vpmullq zmm4,zmm5,zmm6; dec ecx; jnz loop; hlt
    let evex_code = [
        0x62, 0xF2, 0xD5, 0x48, 0x40, 0xE6, 0xFF, 0xC9, 0x75, 0xF6, 0xF4,
    ];
    let setup_evex = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.xmm[5] = [u64::MAX, 0x8000_0000_0000_0000];
        regs.ymm_high[5] = [0x7FFF_FFFF_FFFF_FFFF, 0x0123_4567_89AB_CDEF];
        regs.zmm_high[5] = [0, 1, 2, 0xAAAA_AAAA_AAAA_AAAA];
        regs.xmm[6] = [2, u64::MAX];
        regs.ymm_high[6] = [3, 0xFEDC_BA98_7654_3211];
        regs.zmm_high[6] = [u64::MAX, 7, 0x8000_0000_0000_0000, 3];
        vcpu.set_regs(&regs).unwrap();
        regs
    };
    let (mut interp, _) = make_vcpu_mem(&evex_code);
    let initial = setup_evex(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&evex_code);
    setup_evex(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("EVEX low packed qword multiply JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "EVEX low XMM state");
    assert_eq!(
        jit_regs.ymm_high, interp_regs.ymm_high,
        "EVEX YMM upper state"
    );
    assert_eq!(
        jit_regs.zmm_high, interp_regs.zmm_high,
        "EVEX ZMM upper state"
    );
    assert_eq!(
        jit_regs.xmm[4],
        [
            initial.xmm[5][0].wrapping_mul(initial.xmm[6][0]),
            initial.xmm[5][1].wrapping_mul(initial.xmm[6][1]),
        ]
    );
    assert_eq!(
        jit_regs.ymm_high[4],
        [
            initial.ymm_high[5][0].wrapping_mul(initial.ymm_high[6][0]),
            initial.ymm_high[5][1].wrapping_mul(initial.ymm_high[6][1]),
        ]
    );
    assert_eq!(
        jit_regs.zmm_high[4],
        [
            initial.zmm_high[5][0].wrapping_mul(initial.zmm_high[6][0]),
            initial.zmm_high[5][1].wrapping_mul(initial.zmm_high[6][1]),
            initial.zmm_high[5][2].wrapping_mul(initial.zmm_high[6][2]),
            initial.zmm_high[5][3].wrapping_mul(initial.zmm_high[6][3]),
        ]
    );
}

#[test]
fn jit_packed_integer_absolute_matches_twos_complement_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx2")
        || !std::is_x86_feature_detected!("ssse3")
    {
        return;
    }

    // loop: pabsb xmm1,xmm2; vpabsw ymm3,ymm4; vpabsd xmm5,xmm6;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0x38, 0x1C, 0xCA, 0xC4, 0xE2, 0x7D, 0x1D, 0xDC, 0xC4, 0xE2, 0x79, 0x1E, 0xEE,
        0xFF, 0xC9, 0x75, 0xED, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.xmm[2] = [0x807F_FF01_80FE_027E, 0x8182_8384_7F01_FFFF];
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [1, 2, 3, 4];

        regs.xmm[4] = [0x8000_7FFF_FFFF_0001, 0x8001_1234_EDCC_0000];
        regs.ymm_high[4] = [0xFFFF_8000_0001_7FFF, 0xAAAA_5555_8000_7FFF];
        regs.zmm_high[3] = [5, 6, 7, 8];

        regs.xmm[6] = [0x8000_0000_7FFF_FFFF, 0xFFFF_FFFF_0000_0001];
        regs.ymm_high[5] = [0xDEAD_BEEF_DEAD_BEEF, 0xCAFE_BABE_CAFE_BABE];
        regs.zmm_high[5] = [9, 10, 11, 12];
        vcpu.set_regs(&regs).unwrap();
        regs
    };
    let abs_bytes = |value: u64| {
        let mut result = 0u64;
        for byte in 0..8 {
            let shift = byte * 8;
            result |= u64::from(((value >> shift) as u8 as i8).wrapping_abs() as u8) << shift;
        }
        result
    };
    let abs_words = |value: u64| {
        let mut result = 0u64;
        for word in 0..4 {
            let shift = word * 16;
            result |= u64::from(((value >> shift) as u16 as i16).wrapping_abs() as u16) << shift;
        }
        result
    };
    let abs_dwords = |value: u64| {
        u64::from((value as u32 as i32).wrapping_abs() as u32)
            | (u64::from(((value >> 32) as u32 as i32).wrapping_abs() as u32) << 32)
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("packed integer absolute JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(
        jit_regs.xmm[1],
        [abs_bytes(initial.xmm[2][0]), abs_bytes(initial.xmm[2][1])],
        "legacy PABSB two's-complement absolute"
    );
    assert_eq!(jit_regs.ymm_high[1], initial.ymm_high[1]);
    assert_eq!(jit_regs.zmm_high[1], initial.zmm_high[1]);
    assert_eq!(
        jit_regs.xmm[3],
        [abs_words(initial.xmm[4][0]), abs_words(initial.xmm[4][1])],
        "VEX VPABSW low lane"
    );
    assert_eq!(
        jit_regs.ymm_high[3],
        [
            abs_words(initial.ymm_high[4][0]),
            abs_words(initial.ymm_high[4][1]),
        ],
        "VEX VPABSW high lane"
    );
    assert_eq!(jit_regs.zmm_high[3], [0; 4]);
    assert_eq!(
        jit_regs.xmm[5],
        [abs_dwords(initial.xmm[6][0]), abs_dwords(initial.xmm[6][1]),],
        "VEX VPABSD"
    );
    assert_eq!(jit_regs.ymm_high[5], [0; 2]);
    assert_eq!(jit_regs.zmm_high[5], [0; 4]);

    // loop: vpabsq zmm7,zmm8; dec ecx; jnz loop; hlt
    let evex_code = [
        0x62, 0xD2, 0xFD, 0x48, 0x1F, 0xF8, 0xFF, 0xC9, 0x75, 0xF6, 0xF4,
    ];
    let setup_evex = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.xmm[8] = [i64::MIN as u64, i64::MAX as u64];
        regs.ymm_high[8] = [(-1i64) as u64, (-0x0123_4567_89AB_CDEFi64) as u64];
        regs.zmm_high[8] = [0, 1, (-2i64) as u64, 0x4000_0000_0000_0000];
        vcpu.set_regs(&regs).unwrap();
        regs
    };
    let abs_qwords = |value: u64| (value as i64).wrapping_abs() as u64;
    let (mut interp, _) = make_vcpu_mem(&evex_code);
    let initial = setup_evex(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&evex_code);
    setup_evex(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("EVEX packed qword absolute JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "EVEX low XMM state");
    assert_eq!(
        jit_regs.ymm_high, interp_regs.ymm_high,
        "EVEX YMM upper state"
    );
    assert_eq!(
        jit_regs.zmm_high, interp_regs.zmm_high,
        "EVEX ZMM upper state"
    );
    assert_eq!(
        jit_regs.xmm[7],
        [abs_qwords(initial.xmm[8][0]), abs_qwords(initial.xmm[8][1])]
    );
    assert_eq!(
        jit_regs.ymm_high[7],
        [
            abs_qwords(initial.ymm_high[8][0]),
            abs_qwords(initial.ymm_high[8][1]),
        ]
    );
    assert_eq!(
        jit_regs.zmm_high[7],
        [
            abs_qwords(initial.zmm_high[8][0]),
            abs_qwords(initial.zmm_high[8][1]),
            abs_qwords(initial.zmm_high[8][2]),
            abs_qwords(initial.zmm_high[8][3]),
        ]
    );
}

#[test]
fn jit_fixed_packed_integer_compares_match_predicates_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx2")
        || !std::is_x86_feature_detected!("sse4.2")
    {
        return;
    }

    // loop: pcmpgtb xmm1,xmm2; vpcmpeqw ymm3,ymm4,ymm5;
    //       vpcmpgtq xmm6,xmm7,xmm8; dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0x64, 0xCA, 0xC5, 0xDD, 0x75, 0xDD, 0xC4, 0xC2, 0x41, 0x37, 0xF0, 0xFF, 0xC9,
        0x75, 0xEF, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;

        regs.xmm[1] = [0x7E81_0201_00FF_7F80, 0xFF00_807F_8182_0101];
        regs.xmm[2] = [0x7F82_0101_00FE_807F, 0xFE01_7F80_8281_0102];
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [1, 2, 3, 4];

        regs.xmm[4] = [0x8000_7FFF_FFFF_0001, 0x1234_EDCC_0000_8001];
        regs.ymm_high[4] = [0xFFFF_8000_0001_7FFF, 0xAAAA_5555_8000_7FFF];
        regs.xmm[5] = [0x8000_7FFE_FFFF_0002, 0x1234_EDCC_0001_8001];
        regs.ymm_high[5] = [0xFFFF_7FFF_0001_7FFF, 0xAAAA_5555_7FFF_8000];
        regs.zmm_high[3] = [5, 6, 7, 8];

        regs.xmm[7] = [i64::MIN as u64, 7];
        regs.xmm[8] = [i64::MAX as u64, (-3i64) as u64];
        regs.ymm_high[6] = [0xDEAD_BEEF_DEAD_BEEF, 0xCAFE_BABE_CAFE_BABE];
        regs.zmm_high[6] = [9, 10, 11, 12];
        vcpu.set_regs(&regs).unwrap();
        regs
    };
    let signed_byte_gt = |lhs: u64, rhs: u64| {
        let mut result = 0u64;
        for lane in 0..8 {
            let shift = lane * 8;
            let lhs = (lhs >> shift) as u8 as i8;
            let rhs = (rhs >> shift) as u8 as i8;
            if lhs > rhs {
                result |= 0xFFu64 << shift;
            }
        }
        result
    };
    let word_eq = |lhs: u64, rhs: u64| {
        let mut result = 0u64;
        for lane in 0..4 {
            let shift = lane * 16;
            if (lhs >> shift) as u16 == (rhs >> shift) as u16 {
                result |= 0xFFFFu64 << shift;
            }
        }
        result
    };
    let signed_qword_gt = |lhs: u64, rhs: u64| {
        if (lhs as i64) > (rhs as i64) {
            u64::MAX
        } else {
            0
        }
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("fixed packed integer compare JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");
    assert_eq!(
        jit_regs.xmm[1],
        [
            signed_byte_gt(initial.xmm[1][0], initial.xmm[2][0]),
            signed_byte_gt(initial.xmm[1][1], initial.xmm[2][1]),
        ],
        "legacy signed byte comparison"
    );
    assert_eq!(jit_regs.ymm_high[1], initial.ymm_high[1]);
    assert_eq!(jit_regs.zmm_high[1], initial.zmm_high[1]);
    assert_eq!(
        jit_regs.xmm[3],
        [
            word_eq(initial.xmm[4][0], initial.xmm[5][0]),
            word_eq(initial.xmm[4][1], initial.xmm[5][1]),
        ],
        "VEX.256 word equality low half"
    );
    assert_eq!(
        jit_regs.ymm_high[3],
        [
            word_eq(initial.ymm_high[4][0], initial.ymm_high[5][0]),
            word_eq(initial.ymm_high[4][1], initial.ymm_high[5][1]),
        ],
        "VEX.256 word equality high half"
    );
    assert_eq!(jit_regs.zmm_high[3], [0; 4]);
    assert_eq!(
        jit_regs.xmm[6],
        [
            signed_qword_gt(initial.xmm[7][0], initial.xmm[8][0]),
            signed_qword_gt(initial.xmm[7][1], initial.xmm[8][1]),
        ],
        "VEX.128 signed qword comparison"
    );
    assert_eq!(jit_regs.ymm_high[6], [0; 2]);
    assert_eq!(jit_regs.zmm_high[6], [0; 4]);
}

#[test]
fn jit_packed_integer_interleaves_match_lane_blocks_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vl")
        || !std::is_x86_feature_detected!("avx2")
    {
        return;
    }

    // loop: punpckhbw xmm1,xmm2; vpunpckldq ymm3,ymm4,ymm5;
    //       vpunpckhqdq zmm6,zmm7,zmm8; {evex} vpunpcklbw xmm9,xmm10,xmm11;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0x68, 0xCA, 0xC5, 0xDD, 0x62, 0xDD, 0x62, 0xD1, 0xC5, 0x48, 0x6D, 0xF0, 0x62,
        0x51, 0x2D, 0x08, 0x60, 0xCB, 0xFF, 0xC9, 0x75, 0xE8, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;

        regs.xmm[1] = [0x1111_2222_3333_4444, 0x807F_00FF_A55A_0102];
        regs.xmm[2] = [0x5555_6666_7777_8888, 0x7F80_FF00_5AA5_0304];
        regs.ymm_high[1] = [0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210];
        regs.zmm_high[1] = [1, 2, 3, 4];

        regs.xmm[4] = [0x1111_1111_0000_0000, 0x3333_3333_2222_2222];
        regs.xmm[5] = [0xBBBB_BBBB_AAAA_AAAA, 0xDDDD_DDDD_CCCC_CCCC];
        regs.ymm_high[4] = [0x5555_5555_4444_4444, 0x7777_7777_6666_6666];
        regs.ymm_high[5] = [0xFFFF_FFFF_EEEE_EEEE, 0x9999_9999_8888_8888];
        regs.zmm_high[3] = [5, 6, 7, 8];

        regs.xmm[7] = [0x7000, 0x7001];
        regs.xmm[8] = [0x8000, 0x8001];
        regs.ymm_high[7] = [0x7002, 0x7003];
        regs.ymm_high[8] = [0x8002, 0x8003];
        regs.zmm_high[7] = [0x7004, 0x7005, 0x7006, 0x7007];
        regs.zmm_high[8] = [0x8004, 0x8005, 0x8006, 0x8007];

        regs.xmm[10] = [0x1716_1514_1312_1110, 0x1F1E_1D1C_1B1A_1918];
        regs.xmm[11] = [0x2726_2524_2322_2120, 0x2F2E_2D2C_2B2A_2928];
        regs.ymm_high[9] = [0x9999_9999_9999_9999, 0x9999_9999_9999_9999];
        regs.zmm_high[9] = [9, 9, 9, 9];
        vcpu.set_regs(&regs).unwrap();
        regs
    };
    let interleave_bytes = |lhs: u64, rhs: u64| {
        let mut result = [0u64; 2];
        for lane in 0..8 {
            let lhs_byte = (lhs >> (lane * 8)) & 0xFF;
            let rhs_byte = (rhs >> (lane * 8)) & 0xFF;
            let output = lane * 2;
            result[output / 8] |= lhs_byte << ((output % 8) * 8);
            result[(output + 1) / 8] |= rhs_byte << (((output + 1) % 8) * 8);
        }
        result
    };
    let unpack_low_dwords = |lhs: u64, rhs: u64| {
        [
            ((rhs & 0xFFFF_FFFF) << 32) | (lhs & 0xFFFF_FFFF),
            (rhs & 0xFFFF_FFFF_0000_0000) | (lhs >> 32),
        ]
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("packed integer interleave JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");
    assert_eq!(
        jit_regs.xmm[1],
        interleave_bytes(initial.xmm[1][1], initial.xmm[2][1]),
        "legacy high-byte interleave"
    );
    assert_eq!(jit_regs.ymm_high[1], initial.ymm_high[1]);
    assert_eq!(jit_regs.zmm_high[1], initial.zmm_high[1]);
    assert_eq!(
        jit_regs.xmm[3],
        unpack_low_dwords(initial.xmm[4][0], initial.xmm[5][0]),
        "VEX.256 low-dword interleave low block"
    );
    assert_eq!(
        jit_regs.ymm_high[3],
        unpack_low_dwords(initial.ymm_high[4][0], initial.ymm_high[5][0]),
        "VEX.256 low-dword interleave high block"
    );
    assert_eq!(jit_regs.zmm_high[3], [0; 4]);
    assert_eq!(jit_regs.xmm[6], [initial.xmm[7][1], initial.xmm[8][1]]);
    assert_eq!(
        jit_regs.ymm_high[6],
        [initial.ymm_high[7][1], initial.ymm_high[8][1]]
    );
    assert_eq!(
        jit_regs.zmm_high[6],
        [
            initial.zmm_high[7][1],
            initial.zmm_high[8][1],
            initial.zmm_high[7][3],
            initial.zmm_high[8][3],
        ]
    );
    assert_eq!(
        jit_regs.xmm[9],
        interleave_bytes(initial.xmm[10][0], initial.xmm[11][0]),
        "EVEX.128 low-byte interleave"
    );
    assert_eq!(jit_regs.ymm_high[9], [0; 2]);
    assert_eq!(jit_regs.zmm_high[9], [0; 4]);
}

#[test]
fn jit_saturating_packs_match_lane_blocks_signedness_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vl")
        || !std::is_x86_feature_detected!("avx2")
    {
        return;
    }

    // loop: packsswb xmm1,xmm2; vpackuswb ymm3,ymm4,ymm5;
    //       vpackssdw zmm6,zmm7,zmm8; {evex} vpackusdw xmm9,xmm10,xmm11;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0x63, 0xCA, 0xC5, 0xDD, 0x67, 0xDD, 0x62, 0xD1, 0x45, 0x48, 0x6B, 0xF0, 0x62,
        0x52, 0x2D, 0x08, 0x2B, 0xCB, 0xFF, 0xC9, 0x75, 0xE8, 0xF4,
    ];

    fn vector_bytes(regs: &Registers, index: usize) -> Vec<u8> {
        regs.xmm[index]
            .iter()
            .chain(regs.ymm_high[index].iter())
            .chain(regs.zmm_high[index].iter())
            .flat_map(|word| word.to_le_bytes())
            .collect()
    }

    fn pack_reference(
        first: &[u8],
        second: &[u8],
        width: usize,
        src_bytes: usize,
        to_unsigned: bool,
    ) -> Vec<u8> {
        let dst_bytes = src_bytes / 2;
        let block_lanes = 16 / src_bytes;
        let source_lanes = width / src_bytes;
        let read_signed = |source: &[u8], lane: usize| -> i64 {
            let offset = lane * src_bytes;
            match src_bytes {
                2 => i64::from(i16::from_le_bytes(
                    source[offset..offset + 2].try_into().unwrap(),
                )),
                4 => i64::from(i32::from_le_bytes(
                    source[offset..offset + 4].try_into().unwrap(),
                )),
                _ => unreachable!(),
            }
        };
        let clamp = |value: i64| -> i64 {
            if to_unsigned {
                value.clamp(0, (1i64 << (dst_bytes * 8)) - 1)
            } else {
                value.clamp(
                    -(1i64 << (dst_bytes * 8 - 1)),
                    (1i64 << (dst_bytes * 8 - 1)) - 1,
                )
            }
        };

        let mut result = Vec::with_capacity(width);
        for block_base in (0..source_lanes).step_by(block_lanes) {
            for source in [first, second] {
                for lane in block_base..block_base + block_lanes {
                    result.extend_from_slice(
                        &clamp(read_signed(source, lane)).to_le_bytes()[..dst_bytes],
                    );
                }
            }
        }
        result
    }

    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.rflags = 0xCD7;

        regs.xmm[1] = [0x0080_007F_FFFF_FF80, 0x7FFF_0100_0001_8000];
        regs.xmm[2] = [0xFF7F_00FF_0000_0081, 0xFE00_0200_FF00_1234];
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [1, 2, 3, 4];

        regs.xmm[4] = [0x0080_007F_FFFF_FF80, 0x7FFF_0100_0001_8000];
        regs.ymm_high[4] = [0x00FE_00FF_0100_FF00, 0x1234_0002_FFFE_0200];
        regs.xmm[5] = [0x0002_0001_0000_FFFF, 0x0400_00FF_0080_FF7F];
        regs.ymm_high[5] = [0x8000_7FFF_0101_00FD, 0xFFFF_0007_0008_0009];
        regs.zmm_high[3] = [5, 6, 7, 8];

        regs.xmm[7] = [0x0000_7FFF_FFFF_8000, 0x0000_8000_FFFF_7FFF];
        regs.ymm_high[7] = [0x0001_0000_FFFE_FFFF, 0x7FFF_FFFF_8000_0000];
        regs.zmm_high[7] = [0x0000_7FFE_FFFF_8001, 0x1234_5678_EDCB_A987, 0, u64::MAX];
        regs.xmm[8] = [0x0001_0000_FFFE_FFFF, 0x7FFF_FFFF_8000_0000];
        regs.ymm_high[8] = [0x0000_8001_FFFF_7FFE, 0x4000_0000_C000_0000];
        regs.zmm_high[8] = [
            0x0000_7FFF_FFFF_8000,
            0x0001_0001_FFFE_FFFE,
            1,
            i64::MIN as u64,
        ];

        regs.xmm[10] = [0x0000_FFFF_FFFF_0000, 0x0001_0000_0000_FFFF];
        regs.xmm[11] = [0x0000_8000_FFFF_7FFF, 0x7FFF_FFFF_8000_0000];
        regs.ymm_high[9] = [0x9999_9999_9999_9999, 0xAAAA_AAAA_AAAA_AAAA];
        regs.zmm_high[9] = [9, 10, 11, 12];
        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("saturating packed-narrow JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");

    let initial1 = vector_bytes(&initial, 1);
    let initial2 = vector_bytes(&initial, 2);
    let mut expected = pack_reference(&initial1, &initial2, 16, 2, false);
    expected.extend_from_slice(&initial1[16..]);
    assert_eq!(
        vector_bytes(&jit_regs, 1),
        expected,
        "legacy PACKSSWB and preserved upper state"
    );

    let initial4 = vector_bytes(&initial, 4);
    let initial5 = vector_bytes(&initial, 5);
    let mut expected = pack_reference(&initial4, &initial5, 32, 2, true);
    expected.resize(64, 0);
    assert_eq!(
        vector_bytes(&jit_regs, 3),
        expected,
        "VEX.256 VPACKUSWB lane groups and upper zeroing"
    );

    let initial7 = vector_bytes(&initial, 7);
    let initial8 = vector_bytes(&initial, 8);
    assert_eq!(
        vector_bytes(&jit_regs, 6),
        pack_reference(&initial7, &initial8, 64, 4, false),
        "EVEX.512 VPACKSSDW lane groups"
    );

    let initial10 = vector_bytes(&initial, 10);
    let initial11 = vector_bytes(&initial, 11);
    let mut expected = pack_reference(&initial10, &initial11, 16, 4, true);
    expected.resize(64, 0);
    assert_eq!(
        vector_bytes(&jit_regs, 9),
        expected,
        "EVEX.128 VPACKUSDW unsigned saturation and upper zeroing"
    );
}

#[test]
fn jit_byte_shuffles_match_zeroing_lane_locality_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vl")
        || !std::is_x86_feature_detected!("avx2")
        || !std::is_x86_feature_detected!("ssse3")
    {
        return;
    }

    // loop: pshufb xmm1,xmm2; vpshufb ymm3,ymm4,ymm5;
    //       vpshufb zmm6,zmm7,zmm8; {evex} vpshufb xmm9,xmm10,xmm11;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0x38, 0x00, 0xCA, 0xC4, 0xE2, 0x5D, 0x00, 0xDD, 0x62, 0xD2, 0x45, 0x48, 0x00,
        0xF0, 0x62, 0x52, 0x2D, 0x08, 0x00, 0xCB, 0xFF, 0xC9, 0x75, 0xE6, 0xF4,
    ];

    fn vector_bytes(regs: &Registers, index: usize) -> Vec<u8> {
        regs.xmm[index]
            .iter()
            .chain(regs.ymm_high[index].iter())
            .chain(regs.zmm_high[index].iter())
            .flat_map(|word| word.to_le_bytes())
            .collect()
    }

    fn set_vector_bytes(regs: &mut Registers, index: usize, bytes: &[u8]) {
        let words = bytes
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        regs.xmm[index].copy_from_slice(&words[..2]);
        regs.ymm_high[index].copy_from_slice(&words[2..4]);
        regs.zmm_high[index].copy_from_slice(&words[4..8]);
    }

    fn shuffle_reference(source: &[u8], control: &[u8], width: usize) -> Vec<u8> {
        (0..width)
            .map(|lane| {
                let selector = control[lane];
                if selector & 0x80 != 0 {
                    0
                } else {
                    source[(lane & !15) + usize::from(selector & 15)]
                }
            })
            .collect()
    }

    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.rflags = 0xCD7;
        let source = std::array::from_fn::<_, 64, _>(|lane| {
            (lane as u8).wrapping_mul(29).wrapping_add(0x31)
        });
        let control_pattern = [
            0x80, 0x0F, 0x00, 0x1F, 0x10, 0x07, 0x08, 0xFF, 0x03, 0x0C, 0x11, 0x8F, 0x05, 0x0A,
            0x0E, 0x01,
        ];
        let control = std::array::from_fn::<_, 64, _>(|lane| control_pattern[lane & 15]);

        for (data, selectors) in [(1, 2), (4, 5), (7, 8), (10, 11)] {
            set_vector_bytes(&mut regs, data, &source);
            set_vector_bytes(&mut regs, selectors, &control);
        }
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [1, 2, 3, 4];
        regs.zmm_high[3] = [5, 6, 7, 8];
        regs.ymm_high[9] = [0x9999_9999_9999_9999, 0xAAAA_AAAA_AAAA_AAAA];
        regs.zmm_high[9] = [9, 10, 11, 12];
        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("packed byte-shuffle JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");

    let initial1 = vector_bytes(&initial, 1);
    let initial2 = vector_bytes(&initial, 2);
    let mut expected = shuffle_reference(&initial1, &initial2, 16);
    expected.extend_from_slice(&initial1[16..]);
    assert_eq!(
        vector_bytes(&jit_regs, 1),
        expected,
        "legacy PSHUFB and preserved upper state"
    );

    let mut expected =
        shuffle_reference(&vector_bytes(&initial, 4), &vector_bytes(&initial, 5), 32);
    expected.resize(64, 0);
    assert_eq!(
        vector_bytes(&jit_regs, 3),
        expected,
        "VEX.256 VPSHUFB lane locality and upper zeroing"
    );

    assert_eq!(
        vector_bytes(&jit_regs, 6),
        shuffle_reference(&vector_bytes(&initial, 7), &vector_bytes(&initial, 8), 64,),
        "EVEX.512 VPSHUFB lane locality"
    );

    let mut expected =
        shuffle_reference(&vector_bytes(&initial, 10), &vector_bytes(&initial, 11), 16);
    expected.resize(64, 0);
    assert_eq!(
        vector_bytes(&jit_regs, 9),
        expected,
        "EVEX.128 VPSHUFB upper zeroing"
    );
}

#[test]
fn jit_horizontal_integer_family_matches_grouping_wrap_saturation_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx2")
        || !std::is_x86_feature_detected!("ssse3")
    {
        return;
    }

    // loop: phaddw xmm1,xmm2; vphaddd ymm3,ymm4,ymm5;
    //       vphaddsw ymm6,ymm7,ymm8; vphsubw ymm9,ymm10,ymm11;
    //       vphsubd ymm12,ymm13,ymm14; vphsubsw xmm15,xmm0,xmm2;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0x38, 0x01, 0xCA, 0xC4, 0xE2, 0x5D, 0x02, 0xDD, 0xC4, 0xC2, 0x45, 0x03, 0xF0,
        0xC4, 0x42, 0x2D, 0x05, 0xCB, 0xC4, 0x42, 0x15, 0x06, 0xE6, 0xC4, 0x62, 0x79, 0x07, 0xFA,
        0xFF, 0xC9, 0x75, 0xDE, 0xF4,
    ];

    fn vector_bytes(regs: &Registers, index: usize) -> Vec<u8> {
        regs.xmm[index]
            .iter()
            .chain(regs.ymm_high[index].iter())
            .chain(regs.zmm_high[index].iter())
            .flat_map(|word| word.to_le_bytes())
            .collect()
    }

    fn set_vector_bytes(regs: &mut Registers, index: usize, bytes: &[u8]) {
        let words = bytes
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        regs.xmm[index].copy_from_slice(&words[..2]);
        regs.ymm_high[index].copy_from_slice(&words[2..4]);
        regs.zmm_high[index].copy_from_slice(&words[4..8]);
    }

    fn i16_bytes(values: &[i16; 32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    fn i32_bytes(values: &[i32; 16]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    fn horizontal_i16(
        first: &[u8],
        second: &[u8],
        width: usize,
        subtract: bool,
        saturating: bool,
    ) -> Vec<u8> {
        let lane = |source: &[u8], index: usize| {
            i16::from_le_bytes(source[index * 2..index * 2 + 2].try_into().unwrap())
        };
        let mut result = Vec::with_capacity(width);
        for block_byte in (0..width).step_by(16) {
            let block_lane = block_byte / 2;
            for source in [first, second] {
                for pair in 0..4 {
                    let lhs = lane(source, block_lane + pair * 2);
                    let rhs = lane(source, block_lane + pair * 2 + 1);
                    let value = if saturating {
                        let wide = if subtract {
                            i32::from(lhs) - i32::from(rhs)
                        } else {
                            i32::from(lhs) + i32::from(rhs)
                        };
                        wide.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
                    } else if subtract {
                        lhs.wrapping_sub(rhs)
                    } else {
                        lhs.wrapping_add(rhs)
                    };
                    result.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
        result
    }

    fn horizontal_i32(first: &[u8], second: &[u8], width: usize, subtract: bool) -> Vec<u8> {
        let lane = |source: &[u8], index: usize| {
            i32::from_le_bytes(source[index * 4..index * 4 + 4].try_into().unwrap())
        };
        let mut result = Vec::with_capacity(width);
        for block_byte in (0..width).step_by(16) {
            let block_lane = block_byte / 4;
            for source in [first, second] {
                for pair in 0..2 {
                    let lhs = lane(source, block_lane + pair * 2);
                    let rhs = lane(source, block_lane + pair * 2 + 1);
                    let value = if subtract {
                        lhs.wrapping_sub(rhs)
                    } else {
                        lhs.wrapping_add(rhs)
                    };
                    result.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
        result
    }

    let words_a = std::array::from_fn::<_, 32, _>(|lane| match lane & 7 {
        0 => i16::MAX,
        1 => 1,
        2 => i16::MIN,
        3 => -1,
        4 => 30_000,
        5 => 10_000,
        6 => -30_000,
        _ => -10_000,
    });
    let words_b = std::array::from_fn::<_, 32, _>(|lane| match lane & 7 {
        0 => i16::MIN,
        1 => 1,
        2 => i16::MAX,
        3 => -1,
        4 => -30_000,
        5 => 10_000,
        6 => 30_000,
        _ => -10_000,
    });
    let dwords_a = std::array::from_fn::<_, 16, _>(|lane| match lane & 3 {
        0 => i32::MAX,
        1 => 1,
        2 => i32::MIN,
        _ => -1,
    });
    let dwords_b = std::array::from_fn::<_, 16, _>(|lane| match lane & 3 {
        0 => 0x6000_0000,
        1 => 0x3000_0000,
        2 => -0x6000_0000,
        _ => -0x3000_0000,
    });
    let words_a = i16_bytes(&words_a);
    let words_b = i16_bytes(&words_b);
    let dwords_a = i32_bytes(&dwords_a);
    let dwords_b = i32_bytes(&dwords_b);

    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.rflags = 0xCD7;
        for (index, bytes) in [
            (0, &words_a),
            (1, &words_a),
            (2, &words_b),
            (4, &dwords_a),
            (5, &dwords_b),
            (7, &words_a),
            (8, &words_b),
            (10, &words_a),
            (11, &words_b),
            (13, &dwords_a),
            (14, &dwords_b),
        ] {
            set_vector_bytes(&mut regs, index, bytes);
        }
        regs.zmm_high[3] = [3, 3, 3, 3];
        regs.zmm_high[6] = [6, 6, 6, 6];
        regs.zmm_high[9] = [9, 9, 9, 9];
        regs.zmm_high[12] = [12, 12, 12, 12];
        regs.ymm_high[15] = [15, 15];
        regs.zmm_high[15] = [15, 15, 15, 15];
        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("packed horizontal integer JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");

    let initial1 = vector_bytes(&initial, 1);
    let initial2 = vector_bytes(&initial, 2);
    let mut expected = horizontal_i16(&initial1, &initial2, 16, false, false);
    expected.extend_from_slice(&initial1[16..]);
    assert_eq!(vector_bytes(&jit_regs, 1), expected, "legacy PHADDW");

    for (dst, first, second, elem_bytes, subtract, saturating, width) in [
        (3, 4, 5, 4, false, false, 32),
        (6, 7, 8, 2, false, true, 32),
        (9, 10, 11, 2, true, false, 32),
        (12, 13, 14, 4, true, false, 32),
        (15, 0, 2, 2, true, true, 16),
    ] {
        let first = vector_bytes(&initial, first);
        let second = vector_bytes(&initial, second);
        let mut expected = if elem_bytes == 2 {
            horizontal_i16(&first, &second, width, subtract, saturating)
        } else {
            horizontal_i32(&first, &second, width, subtract)
        };
        expected.resize(64, 0);
        assert_eq!(
            vector_bytes(&jit_regs, dst),
            expected,
            "horizontal destination {dst}"
        );
    }
}

#[test]
fn jit_pavg_matches_unsigned_rounding_aliases_wig_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vl")
        || !std::is_x86_feature_detected!("avx2")
    {
        return;
    }

    // loop: pavgb xmm1,xmm2; {vex3,w1} vpavgw xmm3,xmm4,xmm3;
    //       vpavgb ymm6,ymm6,ymm8; vpavgw zmm16,zmm17,zmm18;
    //       {evex,w1} vpavgb xmm9,xmm10,xmm11;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0xE0, 0xCA, 0xC4, 0xE1, 0xD9, 0xE3, 0xDB, 0xC4, 0xC1, 0x4D, 0xE0, 0xF0, 0x62,
        0xA1, 0x75, 0x40, 0xE3, 0xC2, 0x62, 0x51, 0xAD, 0x08, 0xE0, 0xCB, 0xFF, 0xC9, 0x75, 0xE2,
        0xF4,
    ];

    fn vector_bytes(regs: &Registers, index: usize) -> Vec<u8> {
        if index < 16 {
            regs.xmm[index]
                .iter()
                .chain(regs.ymm_high[index].iter())
                .chain(regs.zmm_high[index].iter())
                .flat_map(|word| word.to_le_bytes())
                .collect()
        } else {
            regs.zmm_ext[index - 16]
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect()
        }
    }

    fn set_vector_bytes(regs: &mut Registers, index: usize, bytes: &[u8]) {
        let words = bytes
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(words.len(), 8);
        if index < 16 {
            regs.xmm[index].copy_from_slice(&words[..2]);
            regs.ymm_high[index].copy_from_slice(&words[2..4]);
            regs.zmm_high[index].copy_from_slice(&words[4..8]);
        } else {
            regs.zmm_ext[index - 16].copy_from_slice(&words);
        }
    }

    fn average_bytes(first: &[u8], second: &[u8], width: usize) -> Vec<u8> {
        first[..width]
            .iter()
            .zip(&second[..width])
            .map(|(a, b)| ((u16::from(*a) + u16::from(*b) + 1) >> 1) as u8)
            .collect()
    }

    fn average_words(first: &[u8], second: &[u8], width: usize) -> Vec<u8> {
        let word = |source: &[u8], lane: usize| {
            u16::from_le_bytes(source[lane * 2..lane * 2 + 2].try_into().unwrap())
        };
        let mut result = Vec::with_capacity(width);
        for lane in 0..width / 2 {
            let average =
                ((u32::from(word(first, lane)) + u32::from(word(second, lane)) + 1) >> 1) as u16;
            result.extend_from_slice(&average.to_le_bytes());
        }
        result
    }

    let first = std::array::from_fn::<_, 64, _>(|lane| match lane & 7 {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 0x7F,
        4 => 0x80,
        5 => 0xFD,
        6 => 0xFE,
        _ => 0xFF,
    });
    let second = std::array::from_fn::<_, 64, _>(|lane| match lane & 7 {
        0 => 0,
        1 => 0,
        2 => 1,
        3 => 0x80,
        4 => 0x7F,
        5 => 0xFE,
        6 => 0xFF,
        _ => 0xFF,
    });
    let sentinel = std::array::from_fn::<_, 64, _>(|lane| 0xC0u8.wrapping_add(lane as u8));

    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.rflags = 0xCD7;

        set_vector_bytes(&mut regs, 1, &first);
        set_vector_bytes(&mut regs, 2, &second);
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [1, 2, 3, 4];

        set_vector_bytes(&mut regs, 3, &second);
        set_vector_bytes(&mut regs, 4, &first);
        regs.ymm_high[3] = [3; 2];
        regs.zmm_high[3] = [3; 4];

        set_vector_bytes(&mut regs, 6, &first);
        set_vector_bytes(&mut regs, 8, &second);
        regs.zmm_high[6] = [6; 4];

        set_vector_bytes(&mut regs, 16, &sentinel);
        set_vector_bytes(&mut regs, 17, &first);
        set_vector_bytes(&mut regs, 18, &second);

        set_vector_bytes(&mut regs, 9, &sentinel);
        set_vector_bytes(&mut regs, 10, &first);
        set_vector_bytes(&mut regs, 11, &second);

        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("PAVGB/PAVGW/VPAVGB/VPAVGW JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.zmm_ext, interp_regs.zmm_ext, "extended ZMM state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");

    let mut expected = average_bytes(&first, &second, 16);
    expected.extend_from_slice(&vector_bytes(&initial, 1)[16..]);
    assert_eq!(
        vector_bytes(&jit_regs, 1),
        expected,
        "legacy rounded byte average and preserved upper state"
    );

    for (dst, width, words, label) in [
        (3, 16, true, "VEX.W1 destination/source-2 alias"),
        (6, 32, false, "VEX.256 destination/source-1 alias"),
        (16, 64, true, "EVEX.512 high registers"),
        (9, 16, false, "EVEX.W1 narrow upper zeroing"),
    ] {
        let mut expected = if words {
            average_words(&first, &second, width)
        } else {
            average_bytes(&first, &second, width)
        };
        expected.resize(64, 0);
        assert_eq!(vector_bytes(&jit_regs, dst), expected, "{label}");
    }
}

#[test]
fn jit_psign_matches_control_sign_aliases_wig_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx2")
        || !std::is_x86_feature_detected!("ssse3")
    {
        return;
    }

    // loop: psignb xmm1,xmm2; {vex3,w1} vpsignw xmm3,xmm4,xmm3;
    //       vpsignd ymm6,ymm6,ymm8; dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0x38, 0x08, 0xCA, 0xC4, 0xE2, 0xD9, 0x09, 0xDB, 0xC4, 0xC2, 0x4D, 0x0A, 0xF0,
        0xFF, 0xC9, 0x75, 0xED, 0xF4,
    ];

    fn vector_bytes(regs: &Registers, index: usize) -> Vec<u8> {
        regs.xmm[index]
            .iter()
            .chain(regs.ymm_high[index].iter())
            .chain(regs.zmm_high[index].iter())
            .flat_map(|word| word.to_le_bytes())
            .collect()
    }

    fn set_vector_bytes(regs: &mut Registers, index: usize, bytes: &[u8]) {
        let words = bytes
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(words.len(), 8);
        regs.xmm[index].copy_from_slice(&words[..2]);
        regs.ymm_high[index].copy_from_slice(&words[2..4]);
        regs.zmm_high[index].copy_from_slice(&words[4..8]);
    }

    fn sign_bytes(values: &[u8], controls: &[u8], width: usize) -> Vec<u8> {
        values[..width]
            .iter()
            .zip(&controls[..width])
            .map(|(value, control)| match *control as i8 {
                0 => 0,
                control if control < 0 => 0u8.wrapping_sub(*value),
                _ => *value,
            })
            .collect()
    }

    fn sign_words(values: &[u8], controls: &[u8], width: usize) -> Vec<u8> {
        let mut result = Vec::with_capacity(width);
        for lane in 0..width / 2 {
            let base = lane * 2;
            let value = i16::from_le_bytes(values[base..base + 2].try_into().unwrap());
            let control = i16::from_le_bytes(controls[base..base + 2].try_into().unwrap());
            let output = match control {
                0 => 0,
                control if control < 0 => value.wrapping_neg(),
                _ => value,
            };
            result.extend_from_slice(&output.to_le_bytes());
        }
        result
    }

    fn sign_dwords(values: &[u8], controls: &[u8], width: usize) -> Vec<u8> {
        let mut result = Vec::with_capacity(width);
        for lane in 0..width / 4 {
            let base = lane * 4;
            let value = i32::from_le_bytes(values[base..base + 4].try_into().unwrap());
            let control = i32::from_le_bytes(controls[base..base + 4].try_into().unwrap());
            let output = match control {
                0 => 0,
                control if control < 0 => value.wrapping_neg(),
                _ => value,
            };
            result.extend_from_slice(&output.to_le_bytes());
        }
        result
    }

    let byte_values = std::array::from_fn::<_, 64, _>(|lane| match lane & 7 {
        0 => i8::MIN as u8,
        1 => i8::MAX as u8,
        2 => (-1i8) as u8,
        3 => 0,
        4 => 1,
        5 => 2,
        6 => (-2i8) as u8,
        _ => 0x55,
    });
    let byte_controls = std::array::from_fn::<_, 64, _>(|lane| match lane % 3 {
        0 => (-1i8) as u8,
        1 => 0,
        _ => 1,
    });
    let word_values = std::array::from_fn::<_, 32, _>(|lane| match lane & 7 {
        0 => i16::MIN,
        1 => i16::MAX,
        2 => -1,
        3 => 0,
        4 => 1,
        5 => 0x1234,
        6 => -0x1234,
        _ => 2,
    })
    .iter()
    .flat_map(|lane| lane.to_le_bytes())
    .collect::<Vec<_>>();
    let word_controls = std::array::from_fn::<_, 32, _>(|lane| match lane % 3 {
        0 => -1i16,
        1 => 0,
        _ => 1,
    })
    .iter()
    .flat_map(|lane| lane.to_le_bytes())
    .collect::<Vec<_>>();
    let dword_values = std::array::from_fn::<_, 16, _>(|lane| match lane & 7 {
        0 => i32::MIN,
        1 => i32::MAX,
        2 => -1,
        3 => 0,
        4 => 1,
        5 => 0x1234_5678,
        6 => -0x1234_5678,
        _ => 2,
    })
    .iter()
    .flat_map(|lane| lane.to_le_bytes())
    .collect::<Vec<_>>();
    let dword_controls = std::array::from_fn::<_, 16, _>(|lane| match lane % 3 {
        0 => -1i32,
        1 => 0,
        _ => 1,
    })
    .iter()
    .flat_map(|lane| lane.to_le_bytes())
    .collect::<Vec<_>>();

    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.rflags = 0xCD7;

        set_vector_bytes(&mut regs, 1, &byte_values);
        set_vector_bytes(&mut regs, 2, &byte_controls);
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [1, 2, 3, 4];

        set_vector_bytes(&mut regs, 3, &word_controls);
        set_vector_bytes(&mut regs, 4, &word_values);
        regs.ymm_high[3] = [3; 2];
        regs.zmm_high[3] = [3; 4];

        set_vector_bytes(&mut regs, 6, &dword_values);
        set_vector_bytes(&mut regs, 8, &dword_controls);
        regs.zmm_high[6] = [6; 4];

        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("PSIGNB/PSIGNW/PSIGND JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");

    let mut expected = sign_bytes(&byte_values, &byte_controls, 16);
    expected.extend_from_slice(&vector_bytes(&initial, 1)[16..]);
    assert_eq!(
        vector_bytes(&jit_regs, 1),
        expected,
        "legacy signed-byte control, wrapping minimum negation, and upper preservation"
    );

    let mut expected = sign_words(&word_values, &word_controls, 16);
    expected.resize(64, 0);
    assert_eq!(
        vector_bytes(&jit_regs, 3),
        expected,
        "VEX.W1 destination/control alias and narrow upper zeroing"
    );

    let mut expected = sign_dwords(&dword_values, &dword_controls, 32);
    expected.resize(64, 0);
    assert_eq!(
        vector_bytes(&jit_regs, 6),
        expected,
        "VEX.256 destination/value alias and upper zeroing"
    );
}

#[test]
fn jit_packed_integer_minmax_matches_signedness_aliases_wig_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vl")
        || !std::is_x86_feature_detected!("avx2")
        || !std::is_x86_feature_detected!("sse4.1")
    {
        return;
    }

    // loop: pminub xmm1,xmm2; pmaxsd xmm8,xmm9;
    //       {vex3,w1} vpminuw xmm3,xmm4,xmm3;
    //       vpmaxsw ymm6,ymm6,ymm5;
    //       {evex,w1} vpminsb zmm16,zmm17,zmm18;
    //       vpmaxuq zmm19,zmm20,zmm21; vpminsd xmm22,xmm23,xmm24;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0xDA, 0xCA, 0x66, 0x45, 0x0F, 0x38, 0x3D, 0xC1, 0xC4, 0xE2, 0xD9, 0x3A, 0xDB,
        0xC5, 0xCD, 0xEE, 0xF5, 0x62, 0xA2, 0xF5, 0x40, 0x38, 0xC2, 0x62, 0xA2, 0xDD, 0x40, 0x3F,
        0xDD, 0x62, 0x82, 0x45, 0x00, 0x39, 0xF0, 0xFF, 0xC9, 0x75, 0xD7, 0xF4,
    ];

    fn vector_bytes(regs: &Registers, index: usize) -> Vec<u8> {
        if index < 16 {
            regs.xmm[index]
                .iter()
                .chain(regs.ymm_high[index].iter())
                .chain(regs.zmm_high[index].iter())
                .flat_map(|word| word.to_le_bytes())
                .collect()
        } else {
            regs.zmm_ext[index - 16]
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect()
        }
    }

    fn set_vector_bytes(regs: &mut Registers, index: usize, bytes: &[u8]) {
        let words = bytes
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(words.len(), 8);
        if index < 16 {
            regs.xmm[index].copy_from_slice(&words[..2]);
            regs.ymm_high[index].copy_from_slice(&words[2..4]);
            regs.zmm_high[index].copy_from_slice(&words[4..8]);
        } else {
            regs.zmm_ext[index - 16].copy_from_slice(&words);
        }
    }

    fn minmax_lanes(
        first: &[u8],
        second: &[u8],
        width: usize,
        elem_bytes: usize,
        signed: bool,
        maximum: bool,
    ) -> Vec<u8> {
        let bits = elem_bytes * 8;
        let mut result = Vec::with_capacity(width);
        for lane in 0..width / elem_bytes {
            let base = lane * elem_bytes;
            let read = |source: &[u8]| {
                let mut bytes = [0u8; 8];
                bytes[..elem_bytes].copy_from_slice(&source[base..base + elem_bytes]);
                u64::from_le_bytes(bytes)
            };
            let first_value = read(first);
            let second_value = read(second);
            let take_first = if signed {
                let shift = 64 - bits;
                let first_signed = ((first_value << shift) as i64) >> shift;
                let second_signed = ((second_value << shift) as i64) >> shift;
                if maximum {
                    first_signed >= second_signed
                } else {
                    first_signed <= second_signed
                }
            } else if maximum {
                first_value >= second_value
            } else {
                first_value <= second_value
            };
            let selected = if take_first {
                first_value
            } else {
                second_value
            };
            result.extend_from_slice(&selected.to_le_bytes()[..elem_bytes]);
        }
        result
    }

    let unsigned_bytes_a = std::array::from_fn::<_, 64, _>(|lane| match lane & 7 {
        0 => 0,
        1 => 1,
        2 => 0x7F,
        3 => 0x80,
        4 => 0xFE,
        5 => 0xFF,
        6 => 0x55,
        _ => 0xAA,
    });
    let unsigned_bytes_b = std::array::from_fn::<_, 64, _>(|lane| match lane & 7 {
        0 => 0xFF,
        1 => 1,
        2 => 0x80,
        3 => 0x7F,
        4 => 2,
        5 => 0,
        6 => 0xAA,
        _ => 0x55,
    });
    let signed_bytes_a = std::array::from_fn::<_, 64, _>(|lane| match lane & 7 {
        0 => i8::MIN as u8,
        1 => i8::MAX as u8,
        2 => (-1i8) as u8,
        3 => 0,
        4 => 1,
        5 => (-2i8) as u8,
        6 => 0x55,
        _ => 0xAA,
    });
    let signed_bytes_b = std::array::from_fn::<_, 64, _>(|lane| match lane & 7 {
        0 => i8::MAX as u8,
        1 => i8::MIN as u8,
        2 => 1,
        3 => 0,
        4 => (-1i8) as u8,
        5 => 2,
        6 => 0xAA,
        _ => 0x55,
    });
    let unsigned_words_a = std::array::from_fn::<_, 32, _>(|lane| match lane & 7 {
        0 => 0u16,
        1 => 1,
        2 => u16::MAX,
        3 => 0x8000,
        4 => 0x7FFF,
        5 => 2,
        6 => 0x5555,
        _ => 0xAAAA,
    })
    .iter()
    .flat_map(|lane| lane.to_le_bytes())
    .collect::<Vec<_>>();
    let unsigned_words_b = std::array::from_fn::<_, 32, _>(|lane| match lane & 7 {
        0 => u16::MAX,
        1 => 1,
        2 => 0,
        3 => 0x7FFF,
        4 => 0x8000,
        5 => 3,
        6 => 0xAAAA,
        _ => 0x5555,
    })
    .iter()
    .flat_map(|lane| lane.to_le_bytes())
    .collect::<Vec<_>>();
    let signed_words_a = std::array::from_fn::<_, 32, _>(|lane| match lane & 7 {
        0 => i16::MIN,
        1 => i16::MAX,
        2 => -1,
        3 => 0,
        4 => 1,
        5 => -2,
        6 => 0x5555,
        _ => -0x5556,
    })
    .iter()
    .flat_map(|lane| lane.to_le_bytes())
    .collect::<Vec<_>>();
    let signed_words_b = std::array::from_fn::<_, 32, _>(|lane| match lane & 7 {
        0 => i16::MAX,
        1 => i16::MIN,
        2 => 1,
        3 => 0,
        4 => -1,
        5 => 2,
        6 => -0x5556,
        _ => 0x5555,
    })
    .iter()
    .flat_map(|lane| lane.to_le_bytes())
    .collect::<Vec<_>>();
    let signed_dwords_a = std::array::from_fn::<_, 16, _>(|lane| match lane & 7 {
        0 => i32::MIN,
        1 => i32::MAX,
        2 => -1,
        3 => 0,
        4 => 1,
        5 => -2,
        6 => 0x5555_5555,
        _ => -0x5555_5556,
    })
    .iter()
    .flat_map(|lane| lane.to_le_bytes())
    .collect::<Vec<_>>();
    let signed_dwords_b = std::array::from_fn::<_, 16, _>(|lane| match lane & 7 {
        0 => i32::MAX,
        1 => i32::MIN,
        2 => 1,
        3 => 0,
        4 => -1,
        5 => 2,
        6 => -0x5555_5556,
        _ => 0x5555_5555,
    })
    .iter()
    .flat_map(|lane| lane.to_le_bytes())
    .collect::<Vec<_>>();
    let unsigned_qwords_a = [0, 1, u64::MAX, 1 << 63, i64::MAX as u64, 2, 0x5555, 0xAAAA]
        .iter()
        .flat_map(|lane| lane.to_le_bytes())
        .collect::<Vec<_>>();
    let unsigned_qwords_b = [u64::MAX, 1, 0, i64::MAX as u64, 1 << 63, 3, 0xAAAA, 0x5555]
        .iter()
        .flat_map(|lane| lane.to_le_bytes())
        .collect::<Vec<_>>();
    let sentinel = std::array::from_fn::<_, 64, _>(|lane| 0xC0u8.wrapping_add(lane as u8));

    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.rflags = 0xCD7;

        set_vector_bytes(&mut regs, 1, &unsigned_bytes_a);
        set_vector_bytes(&mut regs, 2, &unsigned_bytes_b);
        set_vector_bytes(&mut regs, 8, &signed_dwords_a);
        set_vector_bytes(&mut regs, 9, &signed_dwords_b);
        set_vector_bytes(&mut regs, 3, &unsigned_words_b);
        set_vector_bytes(&mut regs, 4, &unsigned_words_a);
        set_vector_bytes(&mut regs, 6, &signed_words_a);
        set_vector_bytes(&mut regs, 5, &signed_words_b);
        set_vector_bytes(&mut regs, 16, &sentinel);
        set_vector_bytes(&mut regs, 17, &signed_bytes_a);
        set_vector_bytes(&mut regs, 18, &signed_bytes_b);
        set_vector_bytes(&mut regs, 19, &sentinel);
        set_vector_bytes(&mut regs, 20, &unsigned_qwords_a);
        set_vector_bytes(&mut regs, 21, &unsigned_qwords_b);
        set_vector_bytes(&mut regs, 22, &sentinel);
        set_vector_bytes(&mut regs, 23, &signed_dwords_a);
        set_vector_bytes(&mut regs, 24, &signed_dwords_b);
        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("packed integer min/max JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.zmm_ext, interp_regs.zmm_ext, "extended ZMM state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");

    for (dst, first, second, width, elem_bytes, signed, maximum, legacy, label) in [
        (
            1,
            &unsigned_bytes_a[..],
            &unsigned_bytes_b[..],
            16,
            1,
            false,
            false,
            true,
            "legacy PMINUB",
        ),
        (
            8,
            &signed_dwords_a[..],
            &signed_dwords_b[..],
            16,
            4,
            true,
            true,
            true,
            "legacy PMAXSD",
        ),
        (
            3,
            &unsigned_words_a[..],
            &unsigned_words_b[..],
            16,
            2,
            false,
            false,
            false,
            "VEX.W1 VPMINUW destination/source-2 alias",
        ),
        (
            6,
            &signed_words_a[..],
            &signed_words_b[..],
            32,
            2,
            true,
            true,
            false,
            "VEX.256 VPMAXSW destination/source-1 alias",
        ),
        (
            16,
            &signed_bytes_a[..],
            &signed_bytes_b[..],
            64,
            1,
            true,
            false,
            false,
            "EVEX.W1 VPMINSB high registers",
        ),
        (
            19,
            &unsigned_qwords_a[..],
            &unsigned_qwords_b[..],
            64,
            8,
            false,
            true,
            false,
            "EVEX.W1 VPMAXUQ",
        ),
        (
            22,
            &signed_dwords_a[..],
            &signed_dwords_b[..],
            16,
            4,
            true,
            false,
            false,
            "EVEX.128 VPMINSD upper zeroing",
        ),
    ] {
        let mut expected = minmax_lanes(first, second, width, elem_bytes, signed, maximum);
        if legacy {
            expected.extend_from_slice(&vector_bytes(&initial, dst)[width..]);
        } else {
            expected.resize(64, 0);
        }
        assert_eq!(vector_bytes(&jit_regs, dst), expected, "{label}");
    }
}

#[test]
fn jit_phminposuw_matches_unsigned_first_tie_alias_wig_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("sse4.1")
        || !std::is_x86_feature_detected!("avx")
    {
        return;
    }

    // loop: phminposuw xmm1,xmm2; {vex3,w1} vphminposuw xmm3,xmm3;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0x38, 0x41, 0xCA, 0xC4, 0xE2, 0xF9, 0x41, 0xDB, 0xFF, 0xC9, 0x75, 0xF2, 0xF4,
    ];

    fn packed_words(words: [u16; 8]) -> [u64; 2] {
        let bytes = words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<Vec<_>>();
        [
            u64::from_le_bytes(bytes[..8].try_into().unwrap()),
            u64::from_le_bytes(bytes[8..].try_into().unwrap()),
        ]
    }

    let legacy_source = packed_words([u16::MAX, 0x8000, 1, 2, 0xC000, 1, 3, 4]);
    let vex_alias_source = packed_words([0x8000, u16::MAX, 0x7FFF, 0, 0, 1, 2, 3]);
    let legacy_ymm_upper = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
    let legacy_zmm_upper = [1, 2, 3, 4];

    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.rflags = 0xCD7;
        regs.xmm[1] = [0xA1A2_A3A4_A5A6_A7A8, 0xB1B2_B3B4_B5B6_B7B8];
        regs.xmm[2] = legacy_source;
        regs.ymm_high[1] = legacy_ymm_upper;
        regs.zmm_high[1] = legacy_zmm_upper;
        regs.xmm[3] = vex_alias_source;
        regs.ymm_high[3] = [0x3333_3333_3333_3333; 2];
        regs.zmm_high[3] = [0x4444_4444_4444_4444; 4];
        vcpu.set_regs(&regs).unwrap();
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("PHMINPOSUW/VPHMINPOSUW JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.zmm_ext, interp_regs.zmm_ext, "extended ZMM state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");

    assert_eq!(jit_regs.xmm[1], [1 | (2 << 16), 0], "legacy first tie");
    assert_eq!(jit_regs.ymm_high[1], legacy_ymm_upper);
    assert_eq!(jit_regs.zmm_high[1], legacy_zmm_upper);
    assert_eq!(
        jit_regs.xmm[3],
        [3 << 16, 0],
        "VEX destination/source alias and first tie"
    );
    assert_eq!(jit_regs.ymm_high[3], [0; 2], "VEX upper-256 zeroing");
    assert_eq!(jit_regs.zmm_high[3], [0; 4], "VEX upper-512 zeroing");
}

#[test]
fn jit_movd_q_register_forms_match_width_direction_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx")
    {
        return;
    }

    // loop: movd xmm1,eax; movq xmm2,rdx; movd r8d,xmm3; movq r9,xmm4;
    //       vmovd xmm5,r10d; vmovq xmm6,r11; vmovd r12d,xmm7;
    //       vmovq r13,xmm8; EVEX vmovd xmm17,r14d; EVEX vmovq xmm18,r15;
    //       EVEX vmovd ebx,xmm19; EVEX vmovq rsi,xmm20;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0x6E, 0xC8, 0x66, 0x48, 0x0F, 0x6E, 0xD2, 0x66, 0x41, 0x0F, 0x7E, 0xD8, 0x66,
        0x49, 0x0F, 0x7E, 0xE1, 0xC4, 0xC1, 0x79, 0x6E, 0xEA, 0xC4, 0xC1, 0xF9, 0x6E, 0xF3, 0xC4,
        0xC1, 0x79, 0x7E, 0xFC, 0xC4, 0x41, 0xF9, 0x7E, 0xC5, 0x62, 0xC1, 0x7D, 0x08, 0x6E, 0xCE,
        0x62, 0xC1, 0xFD, 0x08, 0x6E, 0xD7, 0x62, 0xE1, 0x7D, 0x08, 0x7E, 0xDB, 0x62, 0xE1, 0xFD,
        0x08, 0x7E, 0xE6, 0xFF, 0xC9, 0x75, 0xBD, 0xF4,
    ];

    fn vector_bytes(regs: &Registers, index: usize) -> Vec<u8> {
        if index < 16 {
            regs.xmm[index]
                .iter()
                .chain(regs.ymm_high[index].iter())
                .chain(regs.zmm_high[index].iter())
                .flat_map(|word| word.to_le_bytes())
                .collect()
        } else {
            regs.zmm_ext[index - 16]
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect()
        }
    }

    fn set_vector_bytes(regs: &mut Registers, index: usize, bytes: &[u8; 64]) {
        let words = bytes
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        if index < 16 {
            regs.xmm[index].copy_from_slice(&words[..2]);
            regs.ymm_high[index].copy_from_slice(&words[2..4]);
            regs.zmm_high[index].copy_from_slice(&words[4..8]);
        } else {
            regs.zmm_ext[index - 16].copy_from_slice(&words);
        }
    }

    let sentinel = std::array::from_fn::<_, 64, _>(|index| 0x80u8.wrapping_add(index as u8));
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xFFFF_FFFF_89AB_CDEF;
        regs.rdx = 0x0123_4567_89AB_CDEF;
        regs.rcx = 1;
        regs.r10 = 0xFFFF_FFFF_CAFE_BABE;
        regs.r11 = 0x8877_6655_4433_2211;
        regs.r14 = 0xFFFF_FFFF_7654_3210;
        regs.r15 = 0xDEAD_BEEF_0123_4567;
        regs.rflags = 0xCD7;

        for index in [1usize, 2, 5, 6, 17, 18] {
            set_vector_bytes(&mut regs, index, &sentinel);
        }
        regs.xmm[3][0] = 0x1122_3344_A1B2_C3D4;
        regs.xmm[4][0] = 0x1020_3040_5060_7080;
        regs.xmm[7][0] = 0x9988_7766_89AB_CDEF;
        regs.xmm[8][0] = 0xF0E1_D2C3_B4A5_9687;
        regs.zmm_ext[3][0] = 0xAABB_CCDD_1357_9BDF;
        regs.zmm_ext[4][0] = 0x0F1E_2D3C_4B5A_6978;
        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();

    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("MOVD/MOVQ/VMOVD/VMOVQ JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.zmm_ext, interp_regs.zmm_ext, "extended ZMM state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");

    for (dst, scalar, width, label) in [
        (1usize, initial.rax, 4usize, "legacy MOVD"),
        (2, initial.rdx, 8, "legacy MOVQ"),
    ] {
        let mut expected = vector_bytes(&initial, dst);
        expected[..16].fill(0);
        expected[..width].copy_from_slice(&scalar.to_le_bytes()[..width]);
        assert_eq!(vector_bytes(&jit_regs, dst), expected, "{label}");
    }
    for (dst, scalar, width, label) in [
        (5usize, initial.r10, 4usize, "VEX VMOVD"),
        (6, initial.r11, 8, "VEX VMOVQ"),
        (17, initial.r14, 4, "EVEX VMOVD high XMM"),
        (18, initial.r15, 8, "EVEX VMOVQ high XMM"),
    ] {
        let mut expected = vec![0u8; 64];
        expected[..width].copy_from_slice(&scalar.to_le_bytes()[..width]);
        assert_eq!(vector_bytes(&jit_regs, dst), expected, "{label}");
    }

    assert_eq!(jit_regs.r8, initial.xmm[3][0] as u32 as u64);
    assert_eq!(jit_regs.r9, initial.xmm[4][0]);
    assert_eq!(jit_regs.r12, initial.xmm[7][0] as u32 as u64);
    assert_eq!(jit_regs.r13, initial.xmm[8][0]);
    assert_eq!(jit_regs.rbx, initial.zmm_ext[3][0] as u32 as u64);
    assert_eq!(jit_regs.rsi, initial.zmm_ext[4][0]);
}

#[test]
fn jit_mov_mask_family_matches_lane_bits_extensions_wig_and_source_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx")
        || !std::is_x86_feature_detected!("avx2")
    {
        return;
    }

    // loop: movmskps eax,xmm1; {rex.w} movmskpd rdx,xmm2;
    //       pmovmskb r8d,xmm9; {vex3,w1} vmovmskps r9d,ymm10;
    //       {vex3,w1} vmovmskpd r10d,ymm11;
    //       {vex3,w1} vpmovmskb r11d,xmm12;
    //       {vex3,w1} vpmovmskb r12d,ymm13; dec ecx; jnz loop; hlt
    let code = [
        0x0F, 0x50, 0xC1, 0x66, 0x48, 0x0F, 0x50, 0xD2, 0x66, 0x45, 0x0F, 0xD7, 0xC1, 0xC4, 0x41,
        0xFC, 0x50, 0xCA, 0xC4, 0x41, 0xFD, 0x50, 0xD3, 0xC4, 0x41, 0xF9, 0xD7, 0xDC, 0xC4, 0x41,
        0xFD, 0xD7, 0xE5, 0xFF, 0xC9, 0x75, 0xDB, 0xF4,
    ];

    fn packed_sign_lanes(mask: u32, lanes: usize, lane_bytes: usize) -> [u64; 4] {
        let mut bytes = [0u8; 32];
        for lane in 0..lanes {
            bytes[lane * lane_bytes] = lane as u8;
            if mask & (1 << lane) != 0 {
                bytes[lane * lane_bytes + lane_bytes - 1] |= 0x80;
            }
        }
        let mut words = [0u64; 4];
        for (word, chunk) in words.iter_mut().zip(bytes.chunks_exact(8)) {
            *word = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        words
    }

    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.rax = u64::MAX;
        regs.rdx = u64::MAX;
        regs.r8 = u64::MAX;
        regs.r9 = u64::MAX;
        regs.r10 = u64::MAX;
        regs.r11 = u64::MAX;
        regs.r12 = u64::MAX;
        regs.rflags = 0xCD7;

        let movmskps = packed_sign_lanes(0b1010, 4, 4);
        regs.xmm[1] = [movmskps[0], movmskps[1]];
        let movmskpd = packed_sign_lanes(0b01, 2, 8);
        regs.xmm[2] = [movmskpd[0], movmskpd[1]];
        let pmovmskb = packed_sign_lanes(0xA55A, 16, 1);
        regs.xmm[9] = [pmovmskb[0], pmovmskb[1]];

        let vmovmskps = packed_sign_lanes(0xB3, 8, 4);
        regs.xmm[10] = [vmovmskps[0], vmovmskps[1]];
        regs.ymm_high[10] = [vmovmskps[2], vmovmskps[3]];
        let vmovmskpd = packed_sign_lanes(0xD, 4, 8);
        regs.xmm[11] = [vmovmskpd[0], vmovmskpd[1]];
        regs.ymm_high[11] = [vmovmskpd[2], vmovmskpd[3]];
        let vpmovmskb_xmm = packed_sign_lanes(0xC33C, 16, 1);
        regs.xmm[12] = [vpmovmskb_xmm[0], vpmovmskb_xmm[1]];
        let vpmovmskb_ymm = packed_sign_lanes(0xA55A_C33C, 32, 1);
        regs.xmm[13] = [vpmovmskb_ymm[0], vpmovmskb_ymm[1]];
        regs.ymm_high[13] = [vpmovmskb_ymm[2], vpmovmskb_ymm[3]];
        vcpu.set_regs(&regs).unwrap();
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("MOVMSK/PMOVMSKB JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM source state");
    assert_eq!(
        jit_regs.ymm_high, interp_regs.ymm_high,
        "YMM upper source state"
    );
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.zmm_ext, interp_regs.zmm_ext, "extended ZMM state");
    assert_eq!(jit_regs.rcx, interp_regs.rcx, "loop counter");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");
    assert_eq!(jit_regs.rax, 0b1010, "legacy MOVMSKPS");
    assert_eq!(jit_regs.rdx, 0b01, "legacy REX.W MOVMSKPD");
    assert_eq!(jit_regs.r8, 0xA55A, "legacy PMOVMSKB");
    assert_eq!(jit_regs.r9, 0xB3, "VEX.W1 VMOVMSKPS");
    assert_eq!(jit_regs.r10, 0xD, "VEX.W1 VMOVMSKPD");
    assert_eq!(jit_regs.r11, 0xC33C, "VEX.W1 VPMOVMSKB xmm");
    assert_eq!(jit_regs.r12, 0xA55A_C33C, "VEX.W1 VPMOVMSKB ymm");
}

#[test]
fn jit_psadbw_matches_unsigned_sums_aliases_wig_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vl")
        || !std::is_x86_feature_detected!("avx2")
    {
        return;
    }

    // loop: psadbw xmm1,xmm2; {vex3,w1} vpsadbw xmm3,xmm4,xmm3;
    //       vpsadbw ymm6,ymm6,ymm8; vpsadbw zmm16,zmm17,zmm18;
    //       {evex,w1} vpsadbw xmm9,xmm10,xmm11;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0xF6, 0xCA, 0xC4, 0xE1, 0xD9, 0xF6, 0xDB, 0xC4, 0xC1, 0x4D, 0xF6, 0xF0, 0x62,
        0xA1, 0x75, 0x40, 0xF6, 0xC2, 0x62, 0x51, 0xAD, 0x08, 0xF6, 0xCB, 0xFF, 0xC9, 0x75, 0xE2,
        0xF4,
    ];

    fn vector_bytes(regs: &Registers, index: usize) -> Vec<u8> {
        if index < 16 {
            regs.xmm[index]
                .iter()
                .chain(regs.ymm_high[index].iter())
                .chain(regs.zmm_high[index].iter())
                .flat_map(|word| word.to_le_bytes())
                .collect()
        } else {
            regs.zmm_ext[index - 16]
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect()
        }
    }

    fn set_vector_bytes(regs: &mut Registers, index: usize, bytes: &[u8]) {
        let words = bytes
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(words.len(), 8);
        if index < 16 {
            regs.xmm[index].copy_from_slice(&words[..2]);
            regs.ymm_high[index].copy_from_slice(&words[2..4]);
            regs.zmm_high[index].copy_from_slice(&words[4..8]);
        } else {
            regs.zmm_ext[index - 16].copy_from_slice(&words);
        }
    }

    fn sad_bytes(first: &[u8], second: &[u8], width: usize) -> Vec<u8> {
        let mut result = Vec::with_capacity(width);
        for (a, b) in first[..width]
            .chunks_exact(8)
            .zip(second[..width].chunks_exact(8))
        {
            let sum = a
                .iter()
                .zip(b)
                .map(|(&x, &y)| u64::from(x.abs_diff(y)))
                .sum::<u64>();
            result.extend_from_slice(&sum.to_le_bytes());
        }
        result
    }

    let mut first =
        std::array::from_fn::<_, 64, _>(|lane| (lane as u8).wrapping_mul(37).wrapping_add(11));
    let mut second =
        std::array::from_fn::<_, 64, _>(|lane| 0xF7u8.wrapping_sub((lane as u8).wrapping_mul(19)));
    // Include the architectural maximum: 8 * |0 - 255| = 2040.
    first[..8].fill(0);
    second[..8].fill(255);
    let sentinel = std::array::from_fn::<_, 64, _>(|lane| 0xC0u8.wrapping_add(lane as u8));

    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.rflags = 0xCD7;

        set_vector_bytes(&mut regs, 1, &first);
        set_vector_bytes(&mut regs, 2, &second);
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [1, 2, 3, 4];

        set_vector_bytes(&mut regs, 3, &second);
        set_vector_bytes(&mut regs, 4, &first);
        regs.ymm_high[3] = [3; 2];
        regs.zmm_high[3] = [3; 4];

        set_vector_bytes(&mut regs, 6, &first);
        set_vector_bytes(&mut regs, 8, &second);
        regs.zmm_high[6] = [6; 4];

        set_vector_bytes(&mut regs, 16, &sentinel);
        set_vector_bytes(&mut regs, 17, &first);
        set_vector_bytes(&mut regs, 18, &second);

        set_vector_bytes(&mut regs, 9, &sentinel);
        set_vector_bytes(&mut regs, 10, &first);
        set_vector_bytes(&mut regs, 11, &second);

        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(jit.jit_try_block().expect("PSADBW/VPSADBW JIT eligibility"));
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.zmm_ext, interp_regs.zmm_ext, "extended ZMM state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");

    let mut expected = sad_bytes(&first, &second, 16);
    expected.extend_from_slice(&vector_bytes(&initial, 1)[16..]);
    assert_eq!(
        vector_bytes(&jit_regs, 1),
        expected,
        "legacy unsigned sums and preserved upper state"
    );

    for (dst, width, label) in [
        (3, 16, "VEX.W1 destination/source-2 alias"),
        (6, 32, "VEX.256 destination/source-1 alias"),
        (16, 64, "EVEX.512 high registers"),
        (9, 16, "EVEX.W1 narrow upper zeroing"),
    ] {
        let mut expected = sad_bytes(&first, &second, width);
        expected.resize(64, 0);
        assert_eq!(vector_bytes(&jit_regs, dst), expected, "{label}");
    }
}

#[test]
fn jit_mpsadbw_matches_block_selectors_aliases_wig_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx2")
        || !std::is_x86_feature_detected!("sse4.1")
    {
        return;
    }

    // loop: mpsadbw xmm1,xmm2,0xE7;
    //       {vex3,w1} vmpsadbw xmm3,xmm4,xmm3,0xFF;
    //       vmpsadbw ymm6,ymm6,ymm8,0x38;
    //       {vex3,w1} vmpsadbw xmm9,xmm10,xmm11,0x02;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0x3A, 0x42, 0xCA, 0xE7, 0xC4, 0xE3, 0xD9, 0x42, 0xDB, 0xFF, 0xC4, 0xC3, 0x4D,
        0x42, 0xF0, 0x38, 0xC4, 0x43, 0xA9, 0x42, 0xCB, 0x02, 0xFF, 0xC9, 0x75, 0xE4, 0xF4,
    ];

    fn vector_bytes(regs: &Registers, index: usize) -> Vec<u8> {
        regs.xmm[index]
            .iter()
            .chain(regs.ymm_high[index].iter())
            .chain(regs.zmm_high[index].iter())
            .flat_map(|word| word.to_le_bytes())
            .collect()
    }

    fn set_vector_bytes(regs: &mut Registers, index: usize, bytes: &[u8]) {
        let words = bytes
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(words.len(), 8);
        regs.xmm[index].copy_from_slice(&words[..2]);
        regs.ymm_high[index].copy_from_slice(&words[2..4]);
        regs.zmm_high[index].copy_from_slice(&words[4..8]);
    }

    fn mpsadbw(first: &[u8], second: &[u8], width: usize, imm: u8) -> Vec<u8> {
        let mut result = Vec::with_capacity(width);
        for block in 0..(width / 16) {
            let (first_select, second_select) = if block == 0 {
                ((((imm >> 2) & 1) * 4) as usize, ((imm & 3) * 4) as usize)
            } else {
                (
                    (((imm >> 5) & 1) * 4) as usize,
                    (((imm >> 3) & 3) * 4) as usize,
                )
            };
            let base = block * 16;
            for output in 0..8 {
                let sum = (0..4)
                    .map(|tap| {
                        u16::from(
                            first[base + first_select + output + tap]
                                .abs_diff(second[base + second_select + tap]),
                        )
                    })
                    .sum::<u16>();
                result.extend_from_slice(&sum.to_le_bytes());
            }
        }
        result
    }

    let mut first =
        std::array::from_fn::<_, 64, _>(|lane| (lane as u8).wrapping_mul(37).wrapping_add(11));
    let mut second =
        std::array::from_fn::<_, 64, _>(|lane| 0xF7u8.wrapping_sub((lane as u8).wrapping_mul(19)));
    // Include the architectural maximum: 4 * |0 - 255| = 1020.
    first[4..8].fill(0);
    second[12..16].fill(255);
    let sentinel = std::array::from_fn::<_, 64, _>(|lane| 0xC0u8.wrapping_add(lane as u8));

    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.rflags = 0xCD7;

        set_vector_bytes(&mut regs, 1, &first);
        set_vector_bytes(&mut regs, 2, &second);
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [1, 2, 3, 4];

        set_vector_bytes(&mut regs, 3, &second);
        set_vector_bytes(&mut regs, 4, &first);
        regs.ymm_high[3] = [3; 2];
        regs.zmm_high[3] = [3; 4];

        set_vector_bytes(&mut regs, 6, &first);
        set_vector_bytes(&mut regs, 8, &second);
        regs.zmm_high[6] = [6; 4];

        set_vector_bytes(&mut regs, 9, &sentinel);
        set_vector_bytes(&mut regs, 10, &first);
        set_vector_bytes(&mut regs, 11, &second);

        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("MPSADBW/VMPSADBW JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");

    let mut expected = mpsadbw(&first, &second, 16, 0xE7);
    expected.extend_from_slice(&vector_bytes(&initial, 1)[16..]);
    assert_eq!(
        vector_bytes(&jit_regs, 1),
        expected,
        "legacy selectors, ignored immediate bits, and preserved upper state"
    );
    assert_eq!(
        u16::from_le_bytes(expected[..2].try_into().unwrap()),
        1020,
        "maximum unsigned four-byte sum"
    );

    for (dst, first, second, width, imm, label) in [
        (
            3,
            &first[..],
            &second[..],
            16,
            0xFF,
            "VEX.W1 destination/source-2 alias",
        ),
        (
            6,
            &first[..],
            &second[..],
            32,
            0x38,
            "VEX.256 destination/source-1 alias and lane-specific selectors",
        ),
        (
            9,
            &first[..],
            &second[..],
            16,
            0x02,
            "VEX.W1 high registers and upper zeroing",
        ),
    ] {
        let mut expected = mpsadbw(first, second, width, imm);
        expected.resize(64, 0);
        assert_eq!(vector_bytes(&jit_regs, dst), expected, "{label}");
    }
}

#[test]
fn jit_maddubs_matches_signed_saturation_aliases_wig_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vl")
        || !std::is_x86_feature_detected!("avx2")
        || !std::is_x86_feature_detected!("ssse3")
    {
        return;
    }

    // loop: pmaddubsw xmm1,xmm2; {vex3,w1} vpmaddubsw xmm3,xmm4,xmm3;
    //       vpmaddubsw ymm6,ymm6,ymm8; vpmaddubsw zmm16,zmm17,zmm18;
    //       {evex,w1} vpmaddubsw xmm9,xmm10,xmm11;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0x38, 0x04, 0xCA, 0xC4, 0xE2, 0xD9, 0x04, 0xDB, 0xC4, 0xC2, 0x4D, 0x04, 0xF0,
        0x62, 0xA2, 0x75, 0x40, 0x04, 0xC2, 0x62, 0x52, 0xAD, 0x08, 0x04, 0xCB, 0xFF, 0xC9, 0x75,
        0xE1, 0xF4,
    ];

    fn vector_bytes(regs: &Registers, index: usize) -> Vec<u8> {
        if index < 16 {
            regs.xmm[index]
                .iter()
                .chain(regs.ymm_high[index].iter())
                .chain(regs.zmm_high[index].iter())
                .flat_map(|word| word.to_le_bytes())
                .collect()
        } else {
            regs.zmm_ext[index - 16]
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect()
        }
    }

    fn set_vector_bytes(regs: &mut Registers, index: usize, bytes: &[u8]) {
        let words = bytes
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(words.len(), 8);
        if index < 16 {
            regs.xmm[index].copy_from_slice(&words[..2]);
            regs.ymm_high[index].copy_from_slice(&words[2..4]);
            regs.zmm_high[index].copy_from_slice(&words[4..8]);
        } else {
            regs.zmm_ext[index - 16].copy_from_slice(&words);
        }
    }

    fn maddubs_reference(unsigned: &[u8], signed: &[u8], width: usize) -> Vec<u8> {
        let mut result = Vec::with_capacity(width);
        for pair in 0..width / 2 {
            let base = pair * 2;
            let sum = i32::from(unsigned[base]) * i32::from(signed[base] as i8)
                + i32::from(unsigned[base + 1]) * i32::from(signed[base + 1] as i8);
            let saturated = sum.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
            result.extend_from_slice(&saturated.to_le_bytes());
        }
        result
    }

    let unsigned = std::array::from_fn::<_, 64, _>(|lane| match lane & 7 {
        0..=3 => 255,
        4 => 200,
        5 => 100,
        6 => 1,
        _ => 2,
    });
    let signed = std::array::from_fn::<_, 64, _>(|lane| match lane & 7 {
        0 | 1 => 127i8 as u8,
        2 | 3 => (-128i8) as u8,
        4 => 127i8 as u8,
        5 => (-128i8) as u8,
        6 => (-1i8) as u8,
        _ => 127i8 as u8,
    });
    let sentinel = std::array::from_fn::<_, 64, _>(|lane| 0xA0u8.wrapping_add(lane as u8));

    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.rflags = 0xCD7;

        set_vector_bytes(&mut regs, 1, &unsigned);
        set_vector_bytes(&mut regs, 2, &signed);
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [1, 2, 3, 4];

        set_vector_bytes(&mut regs, 3, &signed);
        set_vector_bytes(&mut regs, 4, &unsigned);
        regs.ymm_high[3] = [0x3333_3333_3333_3333; 2];
        regs.zmm_high[3] = [3; 4];

        set_vector_bytes(&mut regs, 6, &unsigned);
        set_vector_bytes(&mut regs, 8, &signed);
        regs.zmm_high[6] = [6; 4];

        set_vector_bytes(&mut regs, 16, &sentinel);
        set_vector_bytes(&mut regs, 17, &unsigned);
        set_vector_bytes(&mut regs, 18, &signed);

        set_vector_bytes(&mut regs, 9, &sentinel);
        set_vector_bytes(&mut regs, 10, &unsigned);
        set_vector_bytes(&mut regs, 11, &signed);

        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("PMADDUBSW/VPMADDUBSW JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.zmm_ext, interp_regs.zmm_ext, "extended ZMM state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");

    let mut expected = maddubs_reference(&unsigned, &signed, 16);
    expected.extend_from_slice(&vector_bytes(&initial, 1)[16..]);
    assert_eq!(
        vector_bytes(&jit_regs, 1),
        expected,
        "legacy positive/negative saturation and preserved upper state"
    );

    for (dst, width, label) in [
        (3, 16, "VEX.W1 destination/source-2 alias"),
        (6, 32, "VEX.256 destination/source-1 alias"),
        (16, 64, "EVEX.512 high registers"),
        (9, 16, "EVEX.W1 narrow upper zeroing"),
    ] {
        let mut expected = maddubs_reference(&unsigned, &signed, width);
        expected.resize(64, 0);
        assert_eq!(vector_bytes(&jit_regs, dst), expected, "{label}");
    }
}

#[test]
fn jit_maddwd_matches_wrapping_overflow_aliases_wig_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vl")
        || !std::is_x86_feature_detected!("avx2")
    {
        return;
    }

    // loop: pmaddwd xmm1,xmm2; {vex3,w1} vpmaddwd xmm3,xmm4,xmm3;
    //       vpmaddwd ymm6,ymm6,ymm8; vpmaddwd zmm16,zmm17,zmm18;
    //       {evex,w1} vpmaddwd xmm9,xmm10,xmm11;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0xF5, 0xCA, 0xC4, 0xE1, 0xD9, 0xF5, 0xDB, 0xC4, 0xC1, 0x4D, 0xF5, 0xF0, 0x62,
        0xA1, 0x75, 0x40, 0xF5, 0xC2, 0x62, 0x51, 0xAD, 0x08, 0xF5, 0xCB, 0xFF, 0xC9, 0x75, 0xE2,
        0xF4,
    ];

    fn vector_bytes(regs: &Registers, index: usize) -> Vec<u8> {
        if index < 16 {
            regs.xmm[index]
                .iter()
                .chain(regs.ymm_high[index].iter())
                .chain(regs.zmm_high[index].iter())
                .flat_map(|word| word.to_le_bytes())
                .collect()
        } else {
            regs.zmm_ext[index - 16]
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect()
        }
    }

    fn set_vector_bytes(regs: &mut Registers, index: usize, bytes: &[u8]) {
        let words = bytes
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(words.len(), 8);
        if index < 16 {
            regs.xmm[index].copy_from_slice(&words[..2]);
            regs.ymm_high[index].copy_from_slice(&words[2..4]);
            regs.zmm_high[index].copy_from_slice(&words[4..8]);
        } else {
            regs.zmm_ext[index - 16].copy_from_slice(&words);
        }
    }

    fn word_bytes(words: &[i16; 32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    fn maddwd_reference(first: &[u8], second: &[u8], width: usize) -> Vec<u8> {
        let word = |source: &[u8], lane: usize| {
            i16::from_le_bytes(source[lane * 2..lane * 2 + 2].try_into().unwrap())
        };
        let mut result = Vec::with_capacity(width);
        for lane in 0..width / 4 {
            let base = lane * 2;
            let low = i32::from(word(first, base)).wrapping_mul(i32::from(word(second, base)));
            let high =
                i32::from(word(first, base + 1)).wrapping_mul(i32::from(word(second, base + 1)));
            result.extend_from_slice(&low.wrapping_add(high).to_le_bytes());
        }
        result
    }

    let first = std::array::from_fn::<_, 32, _>(|lane| match lane & 7 {
        0 | 1 => i16::MIN,
        2 => i16::MAX,
        3 => -1,
        4 => 30_000,
        5 => -20_000,
        6 => 123,
        _ => -321,
    });
    let second = std::array::from_fn::<_, 32, _>(|lane| match lane & 7 {
        0 | 1 => i16::MIN,
        2 => -2,
        3 => i16::MAX,
        4 => -20_000,
        5 => 30_000,
        6 => -456,
        _ => 789,
    });
    let first = word_bytes(&first);
    let second = word_bytes(&second);
    let sentinel = std::array::from_fn::<_, 64, _>(|lane| 0xB0u8.wrapping_add(lane as u8));
    assert_eq!(
        &maddwd_reference(&first, &second, 4),
        &0x8000_0000u32.to_le_bytes(),
        "double minimum-word product must wrap to INT32_MIN"
    );

    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.rflags = 0xCD7;

        set_vector_bytes(&mut regs, 1, &first);
        set_vector_bytes(&mut regs, 2, &second);
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [1, 2, 3, 4];

        set_vector_bytes(&mut regs, 3, &second);
        set_vector_bytes(&mut regs, 4, &first);
        regs.ymm_high[3] = [0x3333_3333_3333_3333; 2];
        regs.zmm_high[3] = [3; 4];

        set_vector_bytes(&mut regs, 6, &first);
        set_vector_bytes(&mut regs, 8, &second);
        regs.zmm_high[6] = [6; 4];

        set_vector_bytes(&mut regs, 16, &sentinel);
        set_vector_bytes(&mut regs, 17, &first);
        set_vector_bytes(&mut regs, 18, &second);

        set_vector_bytes(&mut regs, 9, &sentinel);
        set_vector_bytes(&mut regs, 10, &first);
        set_vector_bytes(&mut regs, 11, &second);

        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("PMADDWD/VPMADDWD JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.zmm_ext, interp_regs.zmm_ext, "extended ZMM state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");

    let mut expected = maddwd_reference(&first, &second, 16);
    expected.extend_from_slice(&vector_bytes(&initial, 1)[16..]);
    assert_eq!(
        vector_bytes(&jit_regs, 1),
        expected,
        "legacy wrap and preserved upper state"
    );

    for (dst, width, label) in [
        (3, 16, "VEX.W1 destination/source-2 alias"),
        (6, 32, "VEX.256 destination/source-1 alias"),
        (16, 64, "EVEX.512 high registers"),
        (9, 16, "EVEX.W1 narrow upper zeroing"),
    ] {
        let mut expected = maddwd_reference(&first, &second, width);
        expected.resize(64, 0);
        assert_eq!(vector_bytes(&jit_regs, dst), expected, "{label}");
    }
}

#[test]
fn jit_mulhrs_matches_signed_rounding_aliases_wig_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vl")
        || !std::is_x86_feature_detected!("avx2")
        || !std::is_x86_feature_detected!("ssse3")
    {
        return;
    }

    // loop: pmulhrsw xmm1,xmm2; {vex3,w1} vpmulhrsw xmm3,xmm4,xmm3;
    //       vpmulhrsw ymm6,ymm6,ymm8; vpmulhrsw zmm16,zmm17,zmm18;
    //       {evex,w1} vpmulhrsw xmm9,xmm10,xmm11;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0x38, 0x0B, 0xCA, 0xC4, 0xE2, 0xD9, 0x0B, 0xDB, 0xC4, 0xC2, 0x4D, 0x0B, 0xF0,
        0x62, 0xA2, 0x75, 0x40, 0x0B, 0xC2, 0x62, 0x52, 0xAD, 0x08, 0x0B, 0xCB, 0xFF, 0xC9, 0x75,
        0xE1, 0xF4,
    ];

    fn vector_bytes(regs: &Registers, index: usize) -> Vec<u8> {
        if index < 16 {
            regs.xmm[index]
                .iter()
                .chain(regs.ymm_high[index].iter())
                .chain(regs.zmm_high[index].iter())
                .flat_map(|word| word.to_le_bytes())
                .collect()
        } else {
            regs.zmm_ext[index - 16]
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect()
        }
    }

    fn set_vector_bytes(regs: &mut Registers, index: usize, bytes: &[u8]) {
        let words = bytes
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(words.len(), 8);
        if index < 16 {
            regs.xmm[index].copy_from_slice(&words[..2]);
            regs.ymm_high[index].copy_from_slice(&words[2..4]);
            regs.zmm_high[index].copy_from_slice(&words[4..8]);
        } else {
            regs.zmm_ext[index - 16].copy_from_slice(&words);
        }
    }

    fn word_bytes(words: &[i16; 32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    fn mulhrs_reference(first: &[u8], second: &[u8], width: usize) -> Vec<u8> {
        let word = |source: &[u8], lane: usize| {
            i16::from_le_bytes(source[lane * 2..lane * 2 + 2].try_into().unwrap())
        };
        let mut result = Vec::with_capacity(width);
        for lane in 0..width / 2 {
            let product = i32::from(word(first, lane)) * i32::from(word(second, lane));
            let rounded = (product + 0x4000) >> 15;
            result.extend_from_slice(&(rounded as i16).to_le_bytes());
        }
        result
    }

    let first = std::array::from_fn::<_, 32, _>(|lane| match lane & 7 {
        0 => i16::MIN,
        1 => i16::MAX,
        2 => 0x4000,
        3 => -0x4000,
        4 => 0x1234,
        5 => -0x2345,
        6 => 1,
        _ => -1,
    });
    let second = std::array::from_fn::<_, 32, _>(|lane| match lane & 7 {
        0 => i16::MIN,
        1 => i16::MAX,
        2 => 0x4000,
        3 => 0x4000,
        4 => -0x3456,
        5 => -0x4567,
        6 => i16::MAX,
        _ => i16::MIN,
    });
    let first = word_bytes(&first);
    let second = word_bytes(&second);
    let sentinel = std::array::from_fn::<_, 64, _>(|lane| 0xC0u8.wrapping_add(lane as u8));
    assert_eq!(
        &mulhrs_reference(&first, &second, 2),
        &i16::MIN.to_le_bytes(),
        "INT16_MIN squared must wrap the rounded result to INT16_MIN"
    );

    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.rflags = 0xCD7;

        set_vector_bytes(&mut regs, 1, &first);
        set_vector_bytes(&mut regs, 2, &second);
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [1, 2, 3, 4];

        set_vector_bytes(&mut regs, 3, &second);
        set_vector_bytes(&mut regs, 4, &first);
        regs.ymm_high[3] = [0x3333_3333_3333_3333; 2];
        regs.zmm_high[3] = [3; 4];

        set_vector_bytes(&mut regs, 6, &first);
        set_vector_bytes(&mut regs, 8, &second);
        regs.zmm_high[6] = [6; 4];

        set_vector_bytes(&mut regs, 16, &sentinel);
        set_vector_bytes(&mut regs, 17, &first);
        set_vector_bytes(&mut regs, 18, &second);

        set_vector_bytes(&mut regs, 9, &sentinel);
        set_vector_bytes(&mut regs, 10, &first);
        set_vector_bytes(&mut regs, 11, &second);

        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("PMULHRSW/VPMULHRSW JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.zmm_ext, interp_regs.zmm_ext, "extended ZMM state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");

    let mut expected = mulhrs_reference(&first, &second, 16);
    expected.extend_from_slice(&vector_bytes(&initial, 1)[16..]);
    assert_eq!(
        vector_bytes(&jit_regs, 1),
        expected,
        "legacy rounding and preserved upper state"
    );

    for (dst, width, label) in [
        (3, 16, "VEX.W1 destination/source-2 alias"),
        (6, 32, "VEX.256 destination/source-1 alias"),
        (16, 64, "EVEX.512 high registers"),
        (9, 16, "EVEX.W1 narrow upper zeroing"),
    ] {
        let mut expected = mulhrs_reference(&first, &second, width);
        expected.resize(64, 0);
        assert_eq!(vector_bytes(&jit_regs, dst), expected, "{label}");
    }
}

#[test]
fn jit_mulhw_mulhuw_match_signedness_aliases_wig_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vl")
        || !std::is_x86_feature_detected!("avx2")
    {
        return;
    }

    // loop: pmulhw xmm1,xmm2; pmulhuw xmm5,xmm7;
    //       {vex3,w1} vpmulhw xmm3,xmm4,xmm3;
    //       vpmulhuw ymm6,ymm6,ymm8; vpmulhw zmm16,zmm17,zmm18;
    //       {evex,w1} vpmulhuw xmm9,xmm10,xmm11;
    //       dec ecx; jnz loop; hlt
    let code = [
        0x66, 0x0F, 0xE5, 0xCA, 0x66, 0x0F, 0xE4, 0xEF, 0xC4, 0xE1, 0xD9, 0xE5, 0xDB, 0xC4, 0xC1,
        0x4D, 0xE4, 0xF0, 0x62, 0xA1, 0x75, 0x40, 0xE5, 0xC2, 0x62, 0x51, 0xAD, 0x08, 0xE4, 0xCB,
        0xFF, 0xC9, 0x75, 0xDE, 0xF4,
    ];

    fn vector_bytes(regs: &Registers, index: usize) -> Vec<u8> {
        if index < 16 {
            regs.xmm[index]
                .iter()
                .chain(regs.ymm_high[index].iter())
                .chain(regs.zmm_high[index].iter())
                .flat_map(|word| word.to_le_bytes())
                .collect()
        } else {
            regs.zmm_ext[index - 16]
                .iter()
                .flat_map(|word| word.to_le_bytes())
                .collect()
        }
    }

    fn set_vector_bytes(regs: &mut Registers, index: usize, bytes: &[u8]) {
        let words = bytes
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(words.len(), 8);
        if index < 16 {
            regs.xmm[index].copy_from_slice(&words[..2]);
            regs.ymm_high[index].copy_from_slice(&words[2..4]);
            regs.zmm_high[index].copy_from_slice(&words[4..8]);
        } else {
            regs.zmm_ext[index - 16].copy_from_slice(&words);
        }
    }

    fn word_bytes(words: &[u16; 32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    fn mul_high_reference(first: &[u8], second: &[u8], width: usize, signed: bool) -> Vec<u8> {
        let word = |source: &[u8], lane: usize| {
            u16::from_le_bytes(source[lane * 2..lane * 2 + 2].try_into().unwrap())
        };
        let mut result = Vec::with_capacity(width);
        for lane in 0..width / 2 {
            let first = word(first, lane);
            let second = word(second, lane);
            let high = if signed {
                let product = i32::from(first as i16) * i32::from(second as i16);
                (product >> 16) as u16
            } else {
                ((u32::from(first) * u32::from(second)) >> 16) as u16
            };
            result.extend_from_slice(&high.to_le_bytes());
        }
        result
    }

    let first = std::array::from_fn::<_, 32, _>(|lane| match lane & 7 {
        0 => 0x8000,
        1 => 0x7FFF,
        2 => 0xFFFF,
        3 => 0,
        4 => 1,
        5 => 0x8001,
        6 => 0x1234,
        _ => 0xFEDC,
    });
    let second = std::array::from_fn::<_, 32, _>(|lane| match lane & 7 {
        0 => 0x8000,
        1 => 0x8001,
        2 => 0xFFFF,
        3 => 0xFFFF,
        4 => 0x7FFF,
        5 => 0xFFFE,
        6 => 0xFEDC,
        _ => 0x2345,
    });
    let first = word_bytes(&first);
    let second = word_bytes(&second);
    let sentinel = std::array::from_fn::<_, 64, _>(|lane| 0xD0u8.wrapping_add(lane as u8));
    assert_ne!(
        &mul_high_reference(&first, &second, 16, true),
        &mul_high_reference(&first, &second, 16, false),
        "signed and unsigned probes must be observationally distinct"
    );

    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 1;
        regs.rflags = 0xCD7;

        set_vector_bytes(&mut regs, 1, &first);
        set_vector_bytes(&mut regs, 2, &second);
        regs.ymm_high[1] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.zmm_high[1] = [1, 2, 3, 4];

        set_vector_bytes(&mut regs, 5, &first);
        set_vector_bytes(&mut regs, 7, &second);
        regs.ymm_high[5] = [0xAAAA_BBBB_CCCC_DDDD, 0xEEEE_FFFF_0000_1111];
        regs.zmm_high[5] = [5, 6, 7, 8];

        set_vector_bytes(&mut regs, 3, &second);
        set_vector_bytes(&mut regs, 4, &first);
        regs.ymm_high[3] = [3; 2];
        regs.zmm_high[3] = [3; 4];

        set_vector_bytes(&mut regs, 6, &first);
        set_vector_bytes(&mut regs, 8, &second);
        regs.zmm_high[6] = [6; 4];

        set_vector_bytes(&mut regs, 16, &sentinel);
        set_vector_bytes(&mut regs, 17, &first);
        set_vector_bytes(&mut regs, 18, &second);

        set_vector_bytes(&mut regs, 9, &sentinel);
        set_vector_bytes(&mut regs, 10, &first);
        set_vector_bytes(&mut regs, 11, &second);

        vcpu.set_regs(&regs).unwrap();
        regs
    };

    let (mut interp, _) = make_vcpu_mem(&code);
    let initial = setup(&mut interp);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let (mut jit, _) = make_vcpu_mem(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("PMULHW/PMULHUW/VPMULHW/VPMULHUW JIT eligibility")
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();

    assert_eq!(jit_regs.xmm, interp_regs.xmm, "low XMM state");
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high, "YMM upper state");
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high, "ZMM upper state");
    assert_eq!(jit_regs.zmm_ext, interp_regs.zmm_ext, "extended ZMM state");
    assert_eq!(jit_regs.rflags, interp_regs.rflags, "architectural flags");

    for (dst, signed, label) in [
        (1, true, "legacy signed high and preserved upper state"),
        (5, false, "legacy unsigned high and preserved upper state"),
    ] {
        let mut expected = mul_high_reference(&first, &second, 16, signed);
        expected.extend_from_slice(&vector_bytes(&initial, dst)[16..]);
        assert_eq!(vector_bytes(&jit_regs, dst), expected, "{label}");
    }

    for (dst, width, signed, label) in [
        (3, 16, true, "VEX.W1 signed destination/source-2 alias"),
        (6, 32, false, "VEX.256 unsigned destination/source-1 alias"),
        (16, 64, true, "EVEX.512 signed high registers"),
        (9, 16, false, "EVEX.W1 unsigned narrow upper zeroing"),
    ] {
        let mut expected = mul_high_reference(&first, &second, width, signed);
        expected.resize(64, 0);
        assert_eq!(vector_bytes(&jit_regs, dst), expected, "{label}");
    }
}

#[test]
fn jit_vector_memory_moves_match_interpreter_and_fault_at_current_pc() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    const SRC: u64 = 0x20_0000;
    const DST: u64 = 0x21_0000;
    // loop: movaps xmm0,[rbx]; movaps xmm1,xmm0; movaps [rdi],xmm1;
    //       add rbx,16; add rdi,16; dec ecx; jnz loop; hlt
    let code = [
        0x0F, 0x28, 0x03, 0x0F, 0x28, 0xC8, 0x0F, 0x29, 0x0F, 0x48, 0x83, 0xC3, 0x10, 0x48, 0x83,
        0xC7, 0x10, 0xFF, 0xC9, 0x75, 0xEB, 0xF4,
    ];
    let seed: [u8; 32] = std::array::from_fn(|index| (index as u8).wrapping_mul(13) ^ 0xA7);
    let setup = |vcpu: &mut X86_64Vcpu, mem: &Arc<GuestMemoryMmap>| {
        mem.write_slice(&seed, GuestAddress(SRC)).unwrap();
        mem.write_slice(&[0xCC; 32], GuestAddress(DST)).unwrap();
        let mut regs = vcpu.get_regs().unwrap();
        regs.rbx = SRC;
        regs.rdi = DST;
        regs.rcx = 2;
        regs.xmm[0] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];
        regs.ymm_high[0] = [0x9999_AAAA_BBBB_CCCC, 0xDDDD_EEEE_FFFF_0001];
        regs.zmm_high[0] = [2, 3, 4, 5];
        regs.ymm_high[1] = [0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210];
        regs.zmm_high[1] = [6, 7, 8, 9];
        vcpu.set_regs(&regs).unwrap();
    };

    let (mut interp, imem) = make_vcpu_mem(&code);
    setup(&mut interp, &imem);
    run_interp(&mut interp);
    let interp_regs = interp.get_regs().unwrap();
    let mut interp_dst = [0u8; 32];
    imem.read_slice(&mut interp_dst, GuestAddress(DST)).unwrap();

    let (mut jit, jmem) = make_vcpu_mem(&code);
    setup(&mut jit, &jmem);
    jit.set_jit_mem(true);
    assert!(
        jit.jit_try_block().expect("vector-memory jit_try_block"),
        "architectural XMM memory loop should JIT"
    );
    run_interp(&mut jit);
    let jit_regs = jit.get_regs().unwrap();
    let mut jit_dst = [0u8; 32];
    jmem.read_slice(&mut jit_dst, GuestAddress(DST)).unwrap();

    assert_eq!(jit_dst, seed);
    assert_eq!(jit_dst, interp_dst);
    assert_eq!(jit_regs.xmm, interp_regs.xmm);
    assert_eq!(jit_regs.ymm_high, interp_regs.ymm_high);
    assert_eq!(jit_regs.zmm_high, interp_regs.zmm_high);
    assert_eq!(jit_regs.zmm_ext, interp_regs.zmm_ext);
    assert_eq!(jit_regs.rbx, interp_regs.rbx);
    assert_eq!(jit_regs.rdi, interp_regs.rdi);
    assert_eq!(jit_regs.rcx, interp_regs.rcx);
    assert_eq!(jit_regs.rip, interp_regs.rip);

    for (name, opcode, address) in [
        ("misaligned MOVAPS", [0x0F, 0x28, 0x03], SRC + 1),
        ("faulting MOVUPS", [0x0F, 0x10, 0x03], MEM_SIZE + 0x1000),
    ] {
        let fault_code = [
            opcode[0], opcode[1], opcode[2], 0xFF, 0xC9, 0x75, 0xF9, 0xF4,
        ];
        let (mut vcpu, _) = make_vcpu_mem(&fault_code);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rbx = address;
        regs.rcx = 1;
        regs.xmm[0] = [0xA5A5_A5A5_A5A5_A5A5; 2];
        regs.ymm_high[0] = [0x5A5A_5A5A_5A5A_5A5A; 2];
        vcpu.set_regs(&regs).unwrap();
        vcpu.set_jit_mem(true);
        assert!(
            vcpu.jit_try_block()
                .unwrap_or_else(|error| panic!("{name}: {error:?}")),
            "{name}: region should compile before precise deopt"
        );
        let after = vcpu.get_regs().unwrap();
        assert_eq!(after.rip, LOAD_ADDR, "{name}: current-PC restart");
        assert_eq!(after.xmm[0], regs.xmm[0], "{name}: non-committing load");
        assert_eq!(
            after.ymm_high[0], regs.ymm_high[0],
            "{name}: upper-lane preservation"
        );
        assert_eq!(after.rcx, 1, "{name}: later loop ops must not commit");
    }
}

#[test]
fn jit_scalar_memory_source_alu_matches_interpreter_aliases_flags_and_faults() {
    const DATA: u64 = 0x20_0000;

    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        rax: u64,
        rbx: u64,
        r16: u64,
        rflags: u64,
        apx: bool,
    }

    let cases = [
        Case {
            name: "ADD r64,r/m64",
            instruction: &[0x48, 0x03, 0x03],
            rax: 0x7FFF_FFFF_FFFF_FFF0,
            rbx: DATA,
            r16: 0,
            rflags: 0x2,
            apx: false,
        },
        Case {
            name: "OR r64,r/m64",
            instruction: &[0x48, 0x0B, 0x03],
            rax: 0x00FF_0000_F0F0_0000,
            rbx: DATA,
            r16: 0,
            rflags: 0x8D7,
            apx: false,
        },
        Case {
            name: "ADC r64,r/m64",
            instruction: &[0x48, 0x13, 0x03],
            rax: u64::MAX - 4,
            rbx: DATA,
            r16: 0,
            rflags: 0xA03,
            apx: false,
        },
        Case {
            name: "SBB r64,r/m64",
            instruction: &[0x48, 0x1B, 0x03],
            rax: 4,
            rbx: DATA,
            r16: 0,
            rflags: 0xA03,
            apx: false,
        },
        Case {
            name: "AND r64,r/m64",
            instruction: &[0x48, 0x23, 0x03],
            rax: 0xFFFF_0000_FFFF_0000,
            rbx: DATA,
            r16: 0,
            rflags: 0x8D7,
            apx: false,
        },
        Case {
            name: "SUB r64,r/m64",
            instruction: &[0x48, 0x2B, 0x03],
            rax: i64::MIN as u64,
            rbx: DATA,
            r16: 0,
            rflags: 0x202,
            apx: false,
        },
        Case {
            name: "XOR r64,r/m64",
            instruction: &[0x48, 0x33, 0x03],
            rax: 0xAAAA_5555_AAAA_5555,
            rbx: DATA,
            r16: 0,
            rflags: 0x8D7,
            apx: false,
        },
        Case {
            name: "ADD r8,r/m8",
            instruction: &[0x02, 0x03],
            rax: 0x1122_3344_5566_77F0,
            rbx: DATA,
            r16: 0,
            rflags: 0x2,
            apx: false,
        },
        Case {
            name: "ADD r16,r/m16",
            instruction: &[0x66, 0x03, 0x03],
            rax: 0x1122_3344_5566_FFF0,
            rbx: DATA,
            r16: 0,
            rflags: 0x2,
            apx: false,
        },
        Case {
            name: "ADD r32,r/m32",
            instruction: &[0x03, 0x03],
            rax: 0xFFFF_FFFF_FFFF_FFF0,
            rbx: DATA,
            r16: 0,
            rflags: 0x2,
            apx: false,
        },
        Case {
            name: "CMP r64,r/m64",
            instruction: &[0x48, 0x3B, 0x03],
            rax: 0x8000_0000_0000_0000,
            rbx: DATA,
            r16: 0,
            rflags: 0x8D7,
            apx: false,
        },
        Case {
            name: "CMP r/m64,r64",
            instruction: &[0x48, 0x39, 0x03],
            rax: 0x8000_0000_0000_0000,
            rbx: DATA,
            r16: 0,
            rflags: 0x8D7,
            apx: false,
        },
        Case {
            name: "CMP r/m64,imm8",
            instruction: &[0x48, 0x83, 0x3B, 0x7F],
            rax: 0xA5A5_5A5A_A5A5_5A5A,
            rbx: DATA,
            r16: 0,
            rflags: 0x8D7,
            apx: false,
        },
        Case {
            name: "TEST r/m64,r64",
            instruction: &[0x48, 0x85, 0x03],
            rax: 0xF0F0_F0F0_F0F0_F0F0,
            rbx: DATA,
            r16: 0,
            rflags: 0x8D7,
            apx: false,
        },
        Case {
            name: "TEST r/m64,imm32",
            instruction: &[0x48, 0xF7, 0x03, 0x0F, 0x0F, 0x0F, 0x0F],
            rax: 0xA5A5_5A5A_A5A5_5A5A,
            rbx: DATA,
            r16: 0,
            rflags: 0x8D7,
            apx: false,
        },
        Case {
            name: "APX NDD ADD memory rhs",
            instruction: &[0x62, 0xF4, 0x78, 0x18, 0x03, 0x1C, 0x40],
            rax: DATA,
            rbx: 0x7FFF_FFF0,
            r16: 0,
            rflags: 0x2,
            apx: true,
        },
        Case {
            name: "APX NF NDD ADD memory rhs",
            instruction: &[0x62, 0xF4, 0x78, 0x1C, 0x03, 0x1C, 0x40],
            rax: DATA,
            rbx: 0x7FFF_FFF0,
            r16: 0,
            rflags: 0x8D7,
            apx: true,
        },
        Case {
            name: "APX NDD ADD memory lhs",
            instruction: &[0x62, 0xF4, 0x7C, 0x18, 0x01, 0x18],
            rax: DATA,
            rbx: 0x7FFF_FFF0,
            r16: 0,
            rflags: 0x2,
            apx: true,
        },
        Case {
            name: "APX NF NDD ADD memory lhs with destination alias",
            instruction: &[0x62, 0xF4, 0x7C, 0x1C, 0x01, 0x03],
            rax: 0x7FFF_FFF0,
            rbx: DATA,
            r16: 0,
            rflags: 0x8D7,
            apx: true,
        },
    ];

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, memory: &Arc<GuestMemoryMmap>| {
            memory
                .write_obj(0x8000_0000_0000_0011u64, GuestAddress(DATA))
                .unwrap();
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = case.rax;
            regs.rbx = case.rbx;
            regs.r16 = case.r16;
            regs.rflags = case.rflags;
            vcpu.set_regs(&regs).unwrap();
            vcpu.set_apx_enabled(case.apx);
        };

        let (mut interp, interp_mem) = make_vcpu_mem(&code);
        setup(&mut interp, &interp_mem);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the helper-backed native tier",
            case.name
        );
        let actual = jit.get_regs().unwrap();
        let actual_gprs = [
            actual.rax, actual.rcx, actual.rdx, actual.rbx, actual.rsp, actual.rbp, actual.rsi,
            actual.rdi, actual.r8, actual.r9, actual.r10, actual.r11, actual.r12, actual.r13,
            actual.r14, actual.r15, actual.r16, actual.r17, actual.r18, actual.r19, actual.r20,
            actual.r21, actual.r22, actual.r23, actual.r24, actual.r25, actual.r26, actual.r27,
            actual.r28, actual.r29, actual.r30, actual.r31,
        ];
        let expected_gprs = [
            expected.rax,
            expected.rcx,
            expected.rdx,
            expected.rbx,
            expected.rsp,
            expected.rbp,
            expected.rsi,
            expected.rdi,
            expected.r8,
            expected.r9,
            expected.r10,
            expected.r11,
            expected.r12,
            expected.r13,
            expected.r14,
            expected.r15,
            expected.r16,
            expected.r17,
            expected.r18,
            expected.r19,
            expected.r20,
            expected.r21,
            expected.r22,
            expected.r23,
            expected.r24,
            expected.r25,
            expected.r26,
            expected.r27,
            expected.r28,
            expected.r29,
            expected.r30,
            expected.r31,
        ];
        for (index, (actual, expected)) in actual_gprs.into_iter().zip(expected_gprs).enumerate() {
            assert_eq!(actual, expected, "{} GPR index {index}", case.name);
        }
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
        assert_eq!(
            actual.rflags, expected.rflags,
            "{} architectural flags",
            case.name
        );
        assert_eq!(
            jit_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
            0x8000_0000_0000_0011,
            "{} must not modify source memory",
            case.name
        );
    }

    let code = [0x48, 0x03, 0x03, 0xF4]; // add rax,[rbx]; hlt
    let (mut fault, _) = make_vcpu_mem(&code);
    let mut before = fault.get_regs().unwrap();
    before.rax = 0xA5A5_5A5A_A5A5_5A5A;
    before.rbx = MEM_SIZE + 0x1000;
    before.rflags = 0x8D7;
    fault.set_regs(&before).unwrap();
    assert!(
        fault.jit_try_block().expect("faulting scalar memory JIT"),
        "a faulting helper access must compile before precise deoptimization"
    );
    let after = fault.get_regs().unwrap();
    assert_eq!(after.rax, before.rax, "fault must not commit destination");
    assert_eq!(after.rbx, before.rbx, "fault must preserve address base");
    assert_eq!(after.rflags, before.rflags, "fault must preserve flags");
    assert_eq!(after.rip, LOAD_ADDR, "fault must restart at current PC");
}

#[test]
fn jit_scalar_memory_destination_alu_matches_interpreter_widths_sources_and_faults() {
    const DATA: u64 = 0x20_0000;
    const INITIAL: u64 = 0x8000_0000_0000_0011;

    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        rax: u64,
        rbx: u64,
        rsp: u64,
        rbp: u64,
        r16: u64,
        rflags: u64,
        apx: bool,
    }

    let cases = [
        Case {
            name: "ADD r/m64,r64",
            instruction: &[0x48, 0x01, 0x03],
            rax: 0x7FFF_FFFF_FFFF_FFF0,
            rbx: DATA,
            rsp: 0x11_0000,
            rbp: 0xA5A5_5A5A_A5A5_5A5A,
            r16: 0,
            rflags: 0x2,
            apx: false,
        },
        Case {
            name: "OR r/m64,r64",
            instruction: &[0x48, 0x09, 0x03],
            rax: 0x00FF_0000_F0F0_0000,
            rbx: DATA,
            rsp: 0x11_0000,
            rbp: 0,
            r16: 0,
            rflags: 0x8D7,
            apx: false,
        },
        Case {
            name: "ADC r/m64,r64",
            instruction: &[0x48, 0x11, 0x03],
            rax: u64::MAX - 4,
            rbx: DATA,
            rsp: 0x11_0000,
            rbp: 0,
            r16: 0,
            rflags: 0xA03,
            apx: false,
        },
        Case {
            name: "SBB r/m64,r64",
            instruction: &[0x48, 0x19, 0x03],
            rax: 4,
            rbx: DATA,
            rsp: 0x11_0000,
            rbp: 0,
            r16: 0,
            rflags: 0xA03,
            apx: false,
        },
        Case {
            name: "AND r/m64,r64",
            instruction: &[0x48, 0x21, 0x03],
            rax: 0xFFFF_0000_FFFF_0000,
            rbx: DATA,
            rsp: 0x11_0000,
            rbp: 0,
            r16: 0,
            rflags: 0x8D7,
            apx: false,
        },
        Case {
            name: "SUB r/m64,r64",
            instruction: &[0x48, 0x29, 0x03],
            rax: i64::MIN as u64,
            rbx: DATA,
            rsp: 0x11_0000,
            rbp: 0,
            r16: 0,
            rflags: 0x202,
            apx: false,
        },
        Case {
            name: "XOR r/m64,r64",
            instruction: &[0x48, 0x31, 0x03],
            rax: 0xAAAA_5555_AAAA_5555,
            rbx: DATA,
            rsp: 0x11_0000,
            rbp: 0,
            r16: 0,
            rflags: 0x8D7,
            apx: false,
        },
        Case {
            name: "ADD r/m8,r8",
            instruction: &[0x00, 0x03],
            rax: 0x1122_3344_5566_77F0,
            rbx: DATA,
            rsp: 0x11_0000,
            rbp: 0,
            r16: 0,
            rflags: 0x2,
            apx: false,
        },
        Case {
            name: "ADD r/m16,r16",
            instruction: &[0x66, 0x01, 0x03],
            rax: 0x1122_3344_5566_FFF0,
            rbx: DATA,
            rsp: 0x11_0000,
            rbp: 0,
            r16: 0,
            rflags: 0x2,
            apx: false,
        },
        Case {
            name: "ADD r/m32,r32",
            instruction: &[0x01, 0x03],
            rax: 0xFFFF_FFFF_FFFF_FFF0,
            rbx: DATA,
            rsp: 0x11_0000,
            rbp: 0,
            r16: 0,
            rflags: 0x2,
            apx: false,
        },
        Case {
            name: "ADD r/m64,imm8",
            instruction: &[0x48, 0x83, 0x03, 0x7F],
            rax: 0xA5A5_5A5A_A5A5_5A5A,
            rbx: DATA,
            rsp: 0x11_0000,
            rbp: 0,
            r16: 0,
            rflags: 0x8D7,
            apx: false,
        },
        Case {
            name: "ADD r/m64,RSP state source",
            instruction: &[0x48, 0x01, 0x23],
            rax: 0xA5A5_5A5A_A5A5_5A5A,
            rbx: DATA,
            rsp: 0x1234_5678_9ABC_DEF0,
            rbp: 0,
            r16: 0,
            rflags: 0x2,
            apx: false,
        },
        Case {
            name: "ADD r/m8,SPL state source",
            instruction: &[0x40, 0x00, 0x23],
            rax: 0xA5A5_5A5A_A5A5_5A5A,
            rbx: DATA,
            rsp: 0x1234_5678_9ABC_DEF0,
            rbp: 0,
            r16: 0,
            rflags: 0x2,
            apx: false,
        },
        Case {
            name: "ADD r/m64,RBP state source",
            instruction: &[0x48, 0x01, 0x2B],
            rax: 0xA5A5_5A5A_A5A5_5A5A,
            rbx: DATA,
            rsp: 0x11_0000,
            rbp: 0x0123_4567_89AB_CDEF,
            r16: 0,
            rflags: 0x2,
            apx: false,
        },
        Case {
            name: "ADD r/m16,BP state source",
            instruction: &[0x66, 0x01, 0x2B],
            rax: 0xA5A5_5A5A_A5A5_5A5A,
            rbx: DATA,
            rsp: 0x11_0000,
            rbp: 0x0123_4567_89AB_FFF0,
            r16: 0,
            rflags: 0x2,
            apx: false,
        },
        Case {
            name: "REX2 ADD r/m64,R16 state source",
            instruction: &[0xD5, 0x48, 0x01, 0x00],
            rax: DATA,
            rbx: 0xA5A5_5A5A_A5A5_5A5A,
            rsp: 0x11_0000,
            rbp: 0,
            r16: 0x0123_4567_89AB_CDEF,
            rflags: 0x2,
            apx: true,
        },
        Case {
            name: "REX2 ADD r/m32,R16D state source",
            instruction: &[0xD5, 0x40, 0x01, 0x00],
            rax: DATA,
            rbx: 0xA5A5_5A5A_A5A5_5A5A,
            rsp: 0x11_0000,
            rbp: 0,
            r16: 0xFFFF_FFFF_FFFF_FFF0,
            rflags: 0x2,
            apx: true,
        },
        Case {
            name: "ADD [RSP],RSP state address/source alias",
            instruction: &[0x48, 0x01, 0x24, 0x24],
            rax: 0xA5A5_5A5A_A5A5_5A5A,
            rbx: 0x0123_4567_89AB_CDEF,
            rsp: DATA,
            rbp: 0,
            r16: 0,
            rflags: 0x2,
            apx: false,
        },
    ];

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, memory: &Arc<GuestMemoryMmap>| {
            memory.write_obj(INITIAL, GuestAddress(DATA)).unwrap();
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = case.rax;
            regs.rbx = case.rbx;
            regs.rsp = case.rsp;
            regs.rbp = case.rbp;
            regs.r16 = case.r16;
            regs.rflags = case.rflags;
            vcpu.set_regs(&regs).unwrap();
            vcpu.set_apx_enabled(case.apx);
        };

        let (mut interp, interp_mem) = make_vcpu_mem(&code);
        setup(&mut interp, &interp_mem);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        let expected_memory = interp_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap();

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the helper-backed native tier",
            case.name
        );
        let actual = jit.get_regs().unwrap();
        let actual_gprs = [
            actual.rax, actual.rcx, actual.rdx, actual.rbx, actual.rsp, actual.rbp, actual.rsi,
            actual.rdi, actual.r8, actual.r9, actual.r10, actual.r11, actual.r12, actual.r13,
            actual.r14, actual.r15, actual.r16, actual.r17, actual.r18, actual.r19, actual.r20,
            actual.r21, actual.r22, actual.r23, actual.r24, actual.r25, actual.r26, actual.r27,
            actual.r28, actual.r29, actual.r30, actual.r31,
        ];
        let expected_gprs = [
            expected.rax,
            expected.rcx,
            expected.rdx,
            expected.rbx,
            expected.rsp,
            expected.rbp,
            expected.rsi,
            expected.rdi,
            expected.r8,
            expected.r9,
            expected.r10,
            expected.r11,
            expected.r12,
            expected.r13,
            expected.r14,
            expected.r15,
            expected.r16,
            expected.r17,
            expected.r18,
            expected.r19,
            expected.r20,
            expected.r21,
            expected.r22,
            expected.r23,
            expected.r24,
            expected.r25,
            expected.r26,
            expected.r27,
            expected.r28,
            expected.r29,
            expected.r30,
            expected.r31,
        ];
        for (index, (actual, expected)) in actual_gprs.into_iter().zip(expected_gprs).enumerate() {
            assert_eq!(actual, expected, "{} GPR index {index}", case.name);
        }
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
        assert_eq!(
            actual.rflags, expected.rflags,
            "{} architectural flags",
            case.name
        );
        assert_eq!(
            jit_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
            expected_memory,
            "{} memory result",
            case.name
        );
    }

    let code = [0x48, 0x01, 0x03, 0xF4]; // add [rbx],rax; hlt
    let (mut fault, _) = make_vcpu_mem(&code);
    let mut before = fault.get_regs().unwrap();
    before.rax = 0xA5A5_5A5A_A5A5_5A5A;
    before.rbx = MEM_SIZE + 0x1000;
    before.rflags = 0x8D7;
    fault.set_regs(&before).unwrap();
    assert!(
        fault
            .jit_try_block()
            .expect("faulting scalar memory-destination JIT"),
        "an RMW helper access must compile before precise deoptimization"
    );
    let after = fault.get_regs().unwrap();
    assert_eq!(after.rax, before.rax, "load fault must preserve source");
    assert_eq!(
        after.rbx, before.rbx,
        "load fault must preserve address base"
    );
    assert_eq!(
        after.rflags, before.rflags,
        "load fault must preserve flags"
    );
    assert_eq!(
        after.rip, LOAD_ADDR,
        "load fault must restart at current PC"
    );
}

#[test]
fn jit_scalar_memory_destination_unary_matches_interpreter_widths_flags_and_faults() {
    const DATA: u64 = 0x20_0000;

    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        initial: u64,
        rflags: u64,
    }

    let cases = [
        Case {
            name: "NOT r/m8",
            instruction: &[0xF6, 0x13],
            initial: 0x1122_3344_5566_7780,
            rflags: 0x8D7,
        },
        Case {
            name: "NOT r/m16",
            instruction: &[0x66, 0xF7, 0x13],
            initial: 0x1122_3344_5566_8000,
            rflags: 0x8D7,
        },
        Case {
            name: "NOT r/m32",
            instruction: &[0xF7, 0x13],
            initial: 0x1122_3344_8000_0000,
            rflags: 0x8D7,
        },
        Case {
            name: "NOT r/m64",
            instruction: &[0x48, 0xF7, 0x13],
            initial: 0x8000_0000_0000_0000,
            rflags: 0x8D7,
        },
        Case {
            name: "NEG r/m8",
            instruction: &[0xF6, 0x1B],
            initial: 0x1122_3344_5566_7780,
            rflags: 0x202,
        },
        Case {
            name: "NEG r/m16",
            instruction: &[0x66, 0xF7, 0x1B],
            initial: 0x1122_3344_5566_8000,
            rflags: 0x202,
        },
        Case {
            name: "NEG r/m32",
            instruction: &[0xF7, 0x1B],
            initial: 0x1122_3344_8000_0000,
            rflags: 0x202,
        },
        Case {
            name: "NEG r/m64",
            instruction: &[0x48, 0xF7, 0x1B],
            initial: 0x8000_0000_0000_0000,
            rflags: 0x202,
        },
        Case {
            name: "INC r/m8",
            instruction: &[0xFE, 0x03],
            initial: 0x1122_3344_5566_777F,
            rflags: 0xA03,
        },
        Case {
            name: "INC r/m16",
            instruction: &[0x66, 0xFF, 0x03],
            initial: 0x1122_3344_5566_7FFF,
            rflags: 0xA03,
        },
        Case {
            name: "INC r/m32",
            instruction: &[0xFF, 0x03],
            initial: 0x1122_3344_7FFF_FFFF,
            rflags: 0xA03,
        },
        Case {
            name: "INC r/m64",
            instruction: &[0x48, 0xFF, 0x03],
            initial: 0x7FFF_FFFF_FFFF_FFFF,
            rflags: 0xA03,
        },
        Case {
            name: "DEC r/m8",
            instruction: &[0xFE, 0x0B],
            initial: 0x1122_3344_5566_7780,
            rflags: 0xA03,
        },
        Case {
            name: "DEC r/m16",
            instruction: &[0x66, 0xFF, 0x0B],
            initial: 0x1122_3344_5566_8000,
            rflags: 0xA03,
        },
        Case {
            name: "DEC r/m32",
            instruction: &[0xFF, 0x0B],
            initial: 0x1122_3344_8000_0000,
            rflags: 0xA03,
        },
        Case {
            name: "DEC r/m64",
            instruction: &[0x48, 0xFF, 0x0B],
            initial: 0x8000_0000_0000_0000,
            rflags: 0xA03,
        },
    ];

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, memory: &Arc<GuestMemoryMmap>| {
            memory.write_obj(case.initial, GuestAddress(DATA)).unwrap();
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = 0xA5A5_5A5A_A5A5_5A5A;
            regs.rbx = DATA;
            regs.rcx = 0x0123_4567_89AB_CDEF;
            regs.rsp = 0x11_0000;
            regs.rbp = 0x2233_4455_6677_8899;
            regs.rflags = case.rflags;
            vcpu.set_regs(&regs).unwrap();
        };

        let (mut interp, interp_mem) = make_vcpu_mem(&code);
        setup(&mut interp, &interp_mem);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        let expected_memory = interp_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap();

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the helper-backed native tier",
            case.name
        );
        let actual = jit.get_regs().unwrap();
        let actual_gprs = [
            actual.rax, actual.rcx, actual.rdx, actual.rbx, actual.rsp, actual.rbp, actual.rsi,
            actual.rdi, actual.r8, actual.r9, actual.r10, actual.r11, actual.r12, actual.r13,
            actual.r14, actual.r15, actual.r16, actual.r17, actual.r18, actual.r19, actual.r20,
            actual.r21, actual.r22, actual.r23, actual.r24, actual.r25, actual.r26, actual.r27,
            actual.r28, actual.r29, actual.r30, actual.r31,
        ];
        let expected_gprs = [
            expected.rax,
            expected.rcx,
            expected.rdx,
            expected.rbx,
            expected.rsp,
            expected.rbp,
            expected.rsi,
            expected.rdi,
            expected.r8,
            expected.r9,
            expected.r10,
            expected.r11,
            expected.r12,
            expected.r13,
            expected.r14,
            expected.r15,
            expected.r16,
            expected.r17,
            expected.r18,
            expected.r19,
            expected.r20,
            expected.r21,
            expected.r22,
            expected.r23,
            expected.r24,
            expected.r25,
            expected.r26,
            expected.r27,
            expected.r28,
            expected.r29,
            expected.r30,
            expected.r31,
        ];
        for (index, (actual, expected)) in actual_gprs.into_iter().zip(expected_gprs).enumerate() {
            assert_eq!(actual, expected, "{} GPR index {index}", case.name);
        }
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
        assert_eq!(
            actual.rflags, expected.rflags,
            "{} architectural flags",
            case.name
        );
        assert_eq!(
            jit_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
            expected_memory,
            "{} memory result",
            case.name
        );
    }

    let code = [0x48, 0xF7, 0x1B, 0xF4]; // neg qword ptr [rbx]; hlt
    let (mut fault, _) = make_vcpu_mem(&code);
    let mut before = fault.get_regs().unwrap();
    before.rax = 0xA5A5_5A5A_A5A5_5A5A;
    before.rbx = MEM_SIZE + 0x1000;
    before.rflags = 0x8D7;
    fault.set_regs(&before).unwrap();
    assert!(
        fault
            .jit_try_block()
            .expect("faulting scalar memory-destination unary JIT"),
        "a unary RMW helper access must compile before precise deoptimization"
    );
    let after = fault.get_regs().unwrap();
    assert_eq!(after.rax, before.rax, "load fault must preserve scratch");
    assert_eq!(
        after.rbx, before.rbx,
        "load fault must preserve address base"
    );
    assert_eq!(
        after.rflags, before.rflags,
        "load fault must preserve flags"
    );
    assert_eq!(
        after.rip, LOAD_ADDR,
        "load fault must restart at current PC"
    );
}

#[test]
fn jit_scalar_memory_destination_shifts_match_interpreter_counts_flags_and_faults() {
    const DATA: u64 = 0x20_0000;

    struct Case {
        name: &'static str,
        instruction: &'static [u8],
        initial: u64,
        rcx: u64,
        rflags: u64,
    }

    let cases = [
        Case {
            name: "ROL r/m8,1",
            instruction: &[0xD0, 0x03],
            initial: 0x1122_3344_5566_7781,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "ROR r/m16,7",
            instruction: &[0x66, 0xC1, 0x0B, 0x07],
            initial: 0x1122_3344_5566_8001,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "ROL r/m8,8 (effective zero, masked nonzero)",
            instruction: &[0xC0, 0x03, 0x08],
            initial: 0x1122_3344_5566_7780,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "ROR r/m8,9 (effective one, raw multi-bit OF preservation)",
            instruction: &[0xC0, 0x0B, 0x09],
            initial: 0x1122_3344_5566_7702,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "ROL r/m8,CL (effective zero, masked nonzero)",
            instruction: &[0xD2, 0x03],
            initial: 0x1122_3344_5566_7780,
            rcx: 8,
            rflags: 0x8D7,
        },
        Case {
            name: "ROR r/m8,CL (effective one, raw multi-bit OF preservation)",
            instruction: &[0xD2, 0x0B],
            initial: 0x1122_3344_5566_7702,
            rcx: 9,
            rflags: 0x8D7,
        },
        Case {
            name: "ROL r/m16,17 (effective one, raw multi-bit OF preservation)",
            instruction: &[0x66, 0xC1, 0x03, 0x11],
            initial: 0x1122_3344_5566_4000,
            rcx: 0,
            rflags: 0x0D6,
        },
        Case {
            name: "ROR r/m16,17 (effective one, raw multi-bit OF preservation)",
            instruction: &[0x66, 0xC1, 0x0B, 0x11],
            initial: 0x1122_3344_5566_0001,
            rcx: 0,
            rflags: 0x0D6,
        },
        Case {
            name: "ROL r/m16,CL (effective one, raw multi-bit OF preservation)",
            instruction: &[0x66, 0xD3, 0x03],
            initial: 0x1122_3344_5566_4000,
            rcx: 17,
            rflags: 0x0D6,
        },
        Case {
            name: "ROR r/m16,CL (effective zero, masked nonzero)",
            instruction: &[0x66, 0xD3, 0x0B],
            initial: 0x1122_3344_5566_8000,
            rcx: 16,
            rflags: 0x8D6,
        },
        Case {
            name: "RCL r/m8,9 (full through-carry period)",
            instruction: &[0xC0, 0x13, 0x09],
            initial: 0x1122_3344_5566_77A5,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "RCR r/m8,10 (effective one, raw multi-bit OF preservation)",
            instruction: &[0xC0, 0x1B, 0x0A],
            initial: 0x1122_3344_5566_7701,
            rcx: 0,
            rflags: 0x8D6,
        },
        Case {
            name: "RCL r/m16,17 (full through-carry period)",
            instruction: &[0x66, 0xC1, 0x13, 0x11],
            initial: 0x1122_3344_5566_A55A,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "RCR r/m16,18 (effective one, raw multi-bit OF preservation)",
            instruction: &[0x66, 0xC1, 0x1B, 0x12],
            initial: 0x1122_3344_5566_0001,
            rcx: 0,
            rflags: 0x8D6,
        },
        Case {
            name: "RCL r/m8,CL (effective one, raw multi-bit OF preservation)",
            instruction: &[0xD2, 0x13],
            initial: 0x1122_3344_5566_7740,
            rcx: 10,
            rflags: 0x0D6,
        },
        Case {
            name: "RCR r/m8,CL (full through-carry period)",
            instruction: &[0xD2, 0x1B],
            initial: 0x1122_3344_5566_77A5,
            rcx: 9,
            rflags: 0x8D7,
        },
        Case {
            name: "RCL r/m16,CL (effective one, raw multi-bit OF preservation)",
            instruction: &[0x66, 0xD3, 0x13],
            initial: 0x1122_3344_5566_4000,
            rcx: 18,
            rflags: 0x0D6,
        },
        Case {
            name: "RCR r/m16,CL (effective one, raw multi-bit OF preservation)",
            instruction: &[0x66, 0xD3, 0x1B],
            initial: 0x1122_3344_5566_0001,
            rcx: 18,
            rflags: 0x8D6,
        },
        Case {
            name: "RCL r/m32,1",
            instruction: &[0xD1, 0x13],
            initial: 0x1122_3344_8000_0000,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "RCR r/m64,1",
            instruction: &[0x48, 0xD1, 0x1B],
            initial: 0x0000_0000_0000_0001,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "RCL r/m32,32 (masked zero)",
            instruction: &[0xC1, 0x13, 0x20],
            initial: 0x1122_3344_8000_0001,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "RCR r/m64,64 (masked zero)",
            instruction: &[0x48, 0xC1, 0x1B, 0x40],
            initial: 0x8123_4567_89AB_CDEF,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "RCL r/m32,7 (multi-bit OF preservation)",
            instruction: &[0xC1, 0x13, 0x07],
            initial: 0x1122_3344_8000_0001,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "RCR r/m64,7 (multi-bit OF preservation)",
            instruction: &[0x48, 0xC1, 0x1B, 0x07],
            initial: 0x8123_4567_89AB_CDEF,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "ROL r/m32,CL (multi-bit OF preservation)",
            instruction: &[0xD3, 0x03],
            initial: 0x1122_3344_8000_0001,
            rcx: 7,
            rflags: 0x8D7,
        },
        Case {
            name: "RCR r/m64,CL (multi-bit OF preservation)",
            instruction: &[0x48, 0xD3, 0x1B],
            initial: 0x0000_0000_0000_0001,
            rcx: 7,
            rflags: 0x8D7,
        },
        Case {
            name: "ROR r/m64,CL (multi-bit OF preservation)",
            instruction: &[0x48, 0xD3, 0x0B],
            initial: 0x8123_4567_89AB_CDEF,
            rcx: 7,
            rflags: 0x8D7,
        },
        Case {
            name: "SHL r/m32,CL (masked zero)",
            instruction: &[0xD3, 0x23],
            initial: 0x1122_3344_F123_4567,
            rcx: 32,
            rflags: 0x8D7,
        },
        Case {
            name: "SHR r/m64,CL (count one)",
            instruction: &[0x48, 0xD3, 0x2B],
            initial: 0xF123_4567_89AB_CDEF,
            rcx: 1,
            rflags: 0x8D7,
        },
        Case {
            name: "SAR r/m32,CL (multi-bit OF clear)",
            instruction: &[0xD3, 0x3B],
            initial: 0x1122_3344_F123_4567,
            rcx: 7,
            rflags: 0x8D7,
        },
        Case {
            name: "SAR r/m8,8 (operand-width sign fill)",
            instruction: &[0xC0, 0x3B, 0x08],
            initial: 0x1122_3344_5566_7780,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "SAR r/m8,31 (oversized positive sign fill)",
            instruction: &[0xC0, 0x3B, 0x1F],
            initial: 0x1122_3344_5566_777F,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "SAR r/m16,16 (operand-width sign fill)",
            instruction: &[0x66, 0xC1, 0x3B, 0x10],
            initial: 0x1122_3344_5566_8000,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "SAR r/m16,31 (oversized positive sign fill)",
            instruction: &[0x66, 0xC1, 0x3B, 0x1F],
            initial: 0x1122_3344_5566_0001,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "SAR r/m8,CL (oversized negative sign fill)",
            instruction: &[0xD2, 0x3B],
            initial: 0x1122_3344_5566_7780,
            rcx: 31,
            rflags: 0x8D7,
        },
        Case {
            name: "SAR r/m16,CL (oversized positive sign fill)",
            instruction: &[0x66, 0xD3, 0x3B],
            initial: 0x1122_3344_5566_0001,
            rcx: 31,
            rflags: 0x8D7,
        },
        Case {
            name: "SAR r/m8,CL (masked zero)",
            instruction: &[0xD2, 0x3B],
            initial: 0x1122_3344_5566_7780,
            rcx: 32,
            rflags: 0x8D7,
        },
        Case {
            name: "SHL r/m8,32 (masked zero)",
            instruction: &[0xC0, 0x23, 0x20],
            initial: 0x1122_3344_5566_7781,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "SHL r/m16,4",
            instruction: &[0x66, 0xC1, 0x23, 0x04],
            initial: 0x1122_3344_5566_F123,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "SHL r/m8,9 (oversized deterministic CF/OF clear)",
            instruction: &[0xC0, 0x23, 0x09],
            initial: 0x1122_3344_5566_7781,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "SHR r/m8,31 (oversized deterministic CF/OF clear)",
            instruction: &[0xC0, 0x2B, 0x1F],
            initial: 0x1122_3344_5566_7780,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "SHL r/m16,17 (oversized deterministic CF/OF clear)",
            instruction: &[0x66, 0xC1, 0x23, 0x11],
            initial: 0x1122_3344_5566_8001,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "SHR r/m16,31 (oversized deterministic CF/OF clear)",
            instruction: &[0x66, 0xC1, 0x2B, 0x1F],
            initial: 0x1122_3344_5566_8001,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "SHL r/m8,8 (boundary CF clear from original LSB)",
            instruction: &[0xC0, 0x23, 0x08],
            initial: 0x1122_3344_5566_7780,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "SHL r/m8,8 (boundary CF set from original LSB)",
            instruction: &[0xC0, 0x23, 0x08],
            initial: 0x1122_3344_5566_7781,
            rcx: 0,
            rflags: 0x8D6,
        },
        Case {
            name: "SHR r/m16,16 (boundary CF clear from original MSB)",
            instruction: &[0x66, 0xC1, 0x2B, 0x10],
            initial: 0x1122_3344_5566_7FFF,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "SHR r/m16,16 (boundary CF set from original MSB)",
            instruction: &[0x66, 0xC1, 0x2B, 0x10],
            initial: 0x1122_3344_5566_8000,
            rcx: 0,
            rflags: 0x8D6,
        },
        Case {
            name: "SHL r/m8,CL (boundary CF from original LSB)",
            instruction: &[0xD2, 0x23],
            initial: 0x1122_3344_5566_7781,
            rcx: 8,
            rflags: 0x8D6,
        },
        Case {
            name: "SHR r/m16,CL (boundary CF from original MSB)",
            instruction: &[0x66, 0xD3, 0x2B],
            initial: 0x1122_3344_5566_8000,
            rcx: 16,
            rflags: 0x8D6,
        },
        Case {
            name: "SHL r/m8,CL (oversized deterministic CF/OF clear)",
            instruction: &[0xD2, 0x23],
            initial: 0x1122_3344_5566_7781,
            rcx: 31,
            rflags: 0x8D7,
        },
        Case {
            name: "SHR r/m16,CL (oversized deterministic CF/OF clear)",
            instruction: &[0x66, 0xD3, 0x2B],
            initial: 0x1122_3344_5566_8001,
            rcx: 31,
            rflags: 0x8D7,
        },
        Case {
            name: "SHL r/m8,CL (in-range multi-bit)",
            instruction: &[0xD2, 0x23],
            initial: 0x1122_3344_5566_7781,
            rcx: 7,
            rflags: 0x8D7,
        },
        Case {
            name: "SHR r/m16,CL (masked zero)",
            instruction: &[0x66, 0xD3, 0x2B],
            initial: 0x1122_3344_5566_8001,
            rcx: 32,
            rflags: 0x8D7,
        },
        Case {
            name: "SHR r/m32,1",
            instruction: &[0xD1, 0x2B],
            initial: 0x1122_3344_F123_4567,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "SAR r/m64,7",
            instruction: &[0x48, 0xC1, 0x3B, 0x07],
            initial: 0xF123_4567_89AB_CDEF,
            rcx: 0,
            rflags: 0x8D7,
        },
        Case {
            name: "ROL r/m64,9",
            instruction: &[0x48, 0xC1, 0x03, 0x09],
            initial: 0x8123_4567_89AB_CDEF,
            rcx: 0,
            rflags: 0x8D7,
        },
    ];

    for case in cases {
        let mut code = case.instruction.to_vec();
        code.push(0xF4);
        let setup = |vcpu: &mut X86_64Vcpu, memory: &Arc<GuestMemoryMmap>| {
            memory.write_obj(case.initial, GuestAddress(DATA)).unwrap();
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = 0xA5A5_5A5A_A5A5_5A5A;
            regs.rbx = DATA;
            regs.rcx = case.rcx;
            regs.rsp = 0x11_0000;
            regs.rbp = 0x2233_4455_6677_8899;
            regs.rflags = case.rflags;
            vcpu.set_regs(&regs).unwrap();
        };

        let (mut interp, interp_mem) = make_vcpu_mem(&code);
        setup(&mut interp, &interp_mem);
        assert!(
            interp.step().unwrap().is_none(),
            "{} interpreter",
            case.name
        );
        let expected = interp.get_regs().unwrap();
        let expected_memory = interp_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap();

        let (mut jit, jit_mem) = make_vcpu_mem(&code);
        setup(&mut jit, &jit_mem);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.name)),
            "{} must enter the helper-backed native tier",
            case.name
        );
        let actual = jit.get_regs().unwrap();
        let actual_gprs = [
            actual.rax, actual.rcx, actual.rdx, actual.rbx, actual.rsp, actual.rbp, actual.rsi,
            actual.rdi, actual.r8, actual.r9, actual.r10, actual.r11, actual.r12, actual.r13,
            actual.r14, actual.r15, actual.r16, actual.r17, actual.r18, actual.r19, actual.r20,
            actual.r21, actual.r22, actual.r23, actual.r24, actual.r25, actual.r26, actual.r27,
            actual.r28, actual.r29, actual.r30, actual.r31,
        ];
        let expected_gprs = [
            expected.rax,
            expected.rcx,
            expected.rdx,
            expected.rbx,
            expected.rsp,
            expected.rbp,
            expected.rsi,
            expected.rdi,
            expected.r8,
            expected.r9,
            expected.r10,
            expected.r11,
            expected.r12,
            expected.r13,
            expected.r14,
            expected.r15,
            expected.r16,
            expected.r17,
            expected.r18,
            expected.r19,
            expected.r20,
            expected.r21,
            expected.r22,
            expected.r23,
            expected.r24,
            expected.r25,
            expected.r26,
            expected.r27,
            expected.r28,
            expected.r29,
            expected.r30,
            expected.r31,
        ];
        for (index, (actual, expected)) in actual_gprs.into_iter().zip(expected_gprs).enumerate() {
            assert_eq!(actual, expected, "{} GPR index {index}", case.name);
        }
        assert_eq!(actual.rip, expected.rip, "{} RIP", case.name);
        assert_eq!(
            actual.rflags, expected.rflags,
            "{} architectural flags",
            case.name
        );
        assert_eq!(
            jit_mem.read_obj::<u64>(GuestAddress(DATA)).unwrap(),
            expected_memory,
            "{} memory result",
            case.name
        );
    }

    let code = [0x48, 0xC1, 0x23, 0x03, 0xF4]; // shl qword ptr [rbx],3; hlt
    let (mut fault, _) = make_vcpu_mem(&code);
    let mut before = fault.get_regs().unwrap();
    before.rax = 0xA5A5_5A5A_A5A5_5A5A;
    before.rbx = MEM_SIZE + 0x1000;
    before.rcx = 0x0123_4567_89AB_CDEF;
    before.rflags = 0x8D7;
    fault.set_regs(&before).unwrap();
    assert!(
        fault
            .jit_try_block()
            .expect("faulting scalar memory-destination shift JIT"),
        "a shift RMW helper access must compile before precise deoptimization"
    );
    let after = fault.get_regs().unwrap();
    assert_eq!(after.rax, before.rax, "load fault must preserve scratch");
    assert_eq!(after.rbx, before.rbx, "load fault must preserve base");
    assert_eq!(after.rcx, before.rcx, "load fault must preserve RCX");
    assert_eq!(
        after.rflags, before.rflags,
        "load fault must preserve flags"
    );
    assert_eq!(
        after.rip, LOAD_ADDR,
        "load fault must restart at current PC"
    );
}

/// Memory-operand JIT (RAX_JIT_MEM path): a loop that LOADS from a scratch array
/// and STORES each element into a second array runs natively via the MMU helper
/// calls and reproduces the interpreter's GPRs AND memory bit-exact.
#[test]
fn jit_mem_load_store_loop_matches_interpreter() {
    const SCRATCH: u64 = 0x20_0000;
    const DST: u64 = 0x20_0040;
    const COUNT: u32 = 4;

    // mov ecx,COUNT ; mov rbx,SCRATCH ;
    // loop: mov rax,[rbx] ; mov [rbx+0x40],rax ; add rbx,8 ; dec ecx ; jnz loop ; hlt
    let mut code: Vec<u8> = Vec::new();
    code.push(0xB9);
    code.extend_from_slice(&COUNT.to_le_bytes()); // mov ecx, COUNT
    code.extend_from_slice(&[0x48, 0xBB]);
    code.extend_from_slice(&SCRATCH.to_le_bytes()); // mov rbx, SCRATCH (movabs)
    code.extend_from_slice(&[0x48, 0x8B, 0x03]); // mov rax, [rbx]
    code.extend_from_slice(&[0x48, 0x89, 0x43, 0x40]); // mov [rbx+0x40], rax
    code.extend_from_slice(&[0x48, 0x83, 0xC3, 0x08]); // add rbx, 8
    code.extend_from_slice(&[0xFF, 0xC9]); // dec ecx
    code.extend_from_slice(&[0x75, 0xF1]); // jnz loop (rel8 = -15)
    code.push(0xF4); // hlt

    let seed: [u64; 4] = [0x1111_2222_3333_4444, 0xAAAA_BBBB_CCCC_DDDD, 7, 0xDEAD_BEEF];
    let setup = |mem: &Arc<GuestMemoryMmap>| {
        for (i, &val) in seed.iter().enumerate() {
            mem.write_obj(val, GuestAddress(SCRATCH + (i as u64) * 8))
                .unwrap();
            mem.write_obj(0u64, GuestAddress(DST + (i as u64) * 8))
                .unwrap();
        }
    };

    // Interpreter reference.
    let (mut interp, imem) = make_vcpu_mem(&code);
    setup(&imem);
    run_interp(&mut interp);
    let ir = interp.get_regs().unwrap();
    let mut idst = [0u64; 4];
    for (i, slot) in idst.iter_mut().enumerate() {
        *slot = imem.read_obj(GuestAddress(DST + (i as u64) * 8)).unwrap();
    }

    // JIT with memory operands enabled.
    let (mut jit, jmem) = make_vcpu_mem(&code);
    jit.set_jit_mem(true);
    setup(&jmem);
    let ran = jit.jit_try_block().expect("jit_try_block");
    assert!(ran, "the memory loop region should JIT (RAX_JIT_MEM path)");
    run_interp(&mut jit); // step the parked HLT
    let jr = jit.get_regs().unwrap();
    let mut jdst = [0u64; 4];
    for (i, slot) in jdst.iter_mut().enumerate() {
        *slot = jmem.read_obj(GuestAddress(DST + (i as u64) * 8)).unwrap();
    }

    assert_eq!(jr.rax, ir.rax, "rax");
    assert_eq!(jr.rbx, ir.rbx, "rbx");
    assert_eq!(jr.rcx, ir.rcx, "rcx");
    assert_eq!(jdst, idst, "stored array (jit) must match interpreter");
    assert_eq!(jdst, seed, "DST should equal the seed after the copy loop");
    assert_eq!(jr.rbx, SCRATCH + 4 * 8, "rbx walked the array");
}

/// memset-shaped loop: the loop-count flag is set by `dec` BEFORE the stores,
/// and `jnz` reads it AFTER them — so a store that clobbers the flags makes the
/// branch always-taken and the loop overruns. Reproduces the kernel `__memset`
/// region the JIT crashed on. Must terminate and match the interpreter exactly.
#[test]
fn jit_mem_memset_loop_matches_interpreter() {
    const SCRATCH: u64 = 0x20_0000;
    const N: u32 = 5;

    // mov rcx,N ; mov rdi,SCRATCH ; mov rax,0x4242...
    // loop: dec rcx ; mov [rdi],rax ; mov [rdi+8],rax ; lea rdi,[rdi+16] ; jnz loop ; hlt
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&[0x48, 0xC7, 0xC1]);
    code.extend_from_slice(&N.to_le_bytes()); // mov rcx, N
    code.extend_from_slice(&[0x48, 0xBF]);
    code.extend_from_slice(&SCRATCH.to_le_bytes()); // mov rdi, SCRATCH
    code.extend_from_slice(&[0x48, 0xB8]);
    code.extend_from_slice(&0x4242_4242_4242_4242u64.to_le_bytes()); // mov rax, imm
    // loop body (14 bytes): dec rcx(3) ; mov[rdi]rax(3) ; mov[rdi+8]rax(4) ; lea rdi,[rdi+16](4)
    code.extend_from_slice(&[0x48, 0xFF, 0xC9]); // dec rcx
    code.extend_from_slice(&[0x48, 0x89, 0x07]); // mov [rdi], rax
    code.extend_from_slice(&[0x48, 0x89, 0x47, 0x08]); // mov [rdi+8], rax
    code.extend_from_slice(&[0x48, 0x8D, 0x7F, 0x10]); // lea rdi, [rdi+16]
    code.extend_from_slice(&[0x75, 0xF0]); // jnz loop (rel8 = -16)
    code.push(0xF4); // hlt

    let val = 0x4242_4242_4242_4242u64;

    let (mut interp, _imem) = make_vcpu_mem(&code);
    run_interp(&mut interp);
    let ir = interp.get_regs().unwrap();

    let (mut jit, jmem) = make_vcpu_mem(&code);
    jit.set_jit_mem(true);
    let ran = jit.jit_try_block().expect("jit_try_block");
    assert!(ran, "memset loop should JIT");
    run_interp(&mut jit);
    let jr = jit.get_regs().unwrap();

    assert_eq!(
        jr.rcx, ir.rcx,
        "rcx (loop count) — overrun if flags clobbered"
    );
    assert_eq!(jr.rcx, 0, "loop ran exactly N times");
    assert_eq!(jr.rdi, ir.rdi, "rdi");
    assert_eq!(jr.rdi, SCRATCH + (N as u64) * 16, "rdi walked N*16 bytes");
    // The N*2 stored slots are `val`; the slot just past must be untouched (0).
    for i in 0..(N as u64) * 2 {
        let got: u64 = jmem.read_obj(GuestAddress(SCRATCH + i * 8)).unwrap();
        assert_eq!(got, val, "slot {i} stored");
    }
    let past: u64 = jmem
        .read_obj(GuestAddress(SCRATCH + (N as u64) * 16))
        .unwrap();
    assert_eq!(past, 0, "no overrun past the memset region");
}

/// SIB addressing: a copy loop that loads `[rbx + rsi*8]` and stores
/// `[rdi + rsi*8]` exercises the JIT memory path's BaseIndexScale address
/// computation (the most complex addressing mode). Must match the interpreter.
#[test]
fn jit_mem_indexed_copy_loop_matches_interpreter() {
    const SRC: u64 = 0x20_0000;
    const DST: u64 = 0x21_0000;
    const N: u32 = 6;

    // mov rbx,SRC ; mov rdi,DST ; mov ecx,N ; xor esi,esi
    // loop: mov rax,[rbx+rsi*8] ; mov [rdi+rsi*8],rax ; inc rsi ; dec ecx ; jnz loop ; hlt
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&[0x48, 0xBB]);
    code.extend_from_slice(&SRC.to_le_bytes()); // mov rbx, SRC
    code.extend_from_slice(&[0x48, 0xBF]);
    code.extend_from_slice(&DST.to_le_bytes()); // mov rdi, DST
    code.push(0xB9);
    code.extend_from_slice(&N.to_le_bytes()); // mov ecx, N
    code.extend_from_slice(&[0x31, 0xF6]); // xor esi, esi
    // loop body (13 bytes):
    code.extend_from_slice(&[0x48, 0x8B, 0x04, 0xF3]); // mov rax, [rbx+rsi*8]
    code.extend_from_slice(&[0x48, 0x89, 0x04, 0xF7]); // mov [rdi+rsi*8], rax
    code.extend_from_slice(&[0x48, 0xFF, 0xC6]); // inc rsi
    code.extend_from_slice(&[0xFF, 0xC9]); // dec ecx
    code.extend_from_slice(&[0x75, 0xF1]); // jnz loop (rel8 = -15)
    code.push(0xF4); // hlt

    let seed: [u64; 6] = [10, 20, 30, 40, 50, 60];
    let setup = |mem: &Arc<GuestMemoryMmap>| {
        for (i, &v) in seed.iter().enumerate() {
            mem.write_obj(v, GuestAddress(SRC + (i as u64) * 8))
                .unwrap();
            mem.write_obj(0u64, GuestAddress(DST + (i as u64) * 8))
                .unwrap();
        }
    };

    let (mut interp, imem) = make_vcpu_mem(&code);
    setup(&imem);
    run_interp(&mut interp);
    let ir = interp.get_regs().unwrap();

    let (mut jit, jmem) = make_vcpu_mem(&code);
    jit.set_jit_mem(true);
    setup(&jmem);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "indexed copy should JIT"
    );
    run_interp(&mut jit);
    let jr = jit.get_regs().unwrap();

    assert_eq!(jr.rcx, ir.rcx, "rcx");
    assert_eq!(jr.rsi, ir.rsi, "rsi (index)");
    for i in 0..6u64 {
        let got: u64 = jmem.read_obj(GuestAddress(DST + i * 8)).unwrap();
        assert_eq!(got, seed[i as usize], "DST[{i}] via SIB copy");
    }
}

/// Partial-width register stores (B1/B2/B4/B8) and store-immediate forms — the
/// shapes kernel struct/array writers (e.g. text_poke batch entries) use. Each
/// must write exactly `size` bytes via the MMU helper; must match the
/// interpreter's memory image byte-for-byte.
#[test]
fn jit_mem_partial_and_imm_stores_match_interpreter() {
    const DST: u64 = 0x21_0000;
    const STRIDE: u64 = 0x20;
    const N: u32 = 2;
    let rax_val = 0xAABB_CCDD_EEFF_1122u64;

    // mov rdi,DST ; mov rax,rax_val ; mov ecx,N
    // loop:
    //   mov [rdi],al ; mov [rdi+1],ax ; mov [rdi+4],eax ; mov [rdi+8],rax ;
    //   mov byte [rdi+16],0x55 ; mov dword [rdi+20],0x12345678 ;
    //   add rdi,0x20 ; dec ecx ; jnz loop ; hlt
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&[0x48, 0xBF]);
    code.extend_from_slice(&DST.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xB8]);
    code.extend_from_slice(&rax_val.to_le_bytes());
    code.push(0xB9);
    code.extend_from_slice(&N.to_le_bytes());
    // loop body (30 bytes):
    code.extend_from_slice(&[0x88, 0x07]); // mov [rdi], al
    code.extend_from_slice(&[0x66, 0x89, 0x47, 0x01]); // mov [rdi+1], ax
    code.extend_from_slice(&[0x89, 0x47, 0x04]); // mov [rdi+4], eax
    code.extend_from_slice(&[0x48, 0x89, 0x47, 0x08]); // mov [rdi+8], rax
    code.extend_from_slice(&[0xC6, 0x47, 0x10, 0x55]); // mov byte [rdi+16], 0x55
    code.extend_from_slice(&[0xC7, 0x47, 0x14, 0x78, 0x56, 0x34, 0x12]); // mov dword [rdi+20], 0x12345678
    code.extend_from_slice(&[0x48, 0x83, 0xC7, 0x20]); // add rdi, 0x20
    code.extend_from_slice(&[0xFF, 0xC9]); // dec ecx
    code.extend_from_slice(&[0x75, 0xE0]); // jnz loop (rel8 = -32)
    code.push(0xF4); // hlt

    let (mut interp, imem) = make_vcpu_mem(&code);
    for i in 0..(N as u64) * STRIDE {
        imem.write_obj(0u8, GuestAddress(DST + i)).unwrap();
    }
    run_interp(&mut interp);

    let (mut jit, jmem) = make_vcpu_mem(&code);
    jit.set_jit_mem(true);
    for i in 0..(N as u64) * STRIDE {
        jmem.write_obj(0u8, GuestAddress(DST + i)).unwrap();
    }
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "partial-store loop should JIT"
    );
    run_interp(&mut jit);

    // Compare the whole written region byte-for-byte against the interpreter.
    for i in 0..(N as u64) * STRIDE {
        let ib: u8 = imem.read_obj(GuestAddress(DST + i)).unwrap();
        let jb: u8 = jmem.read_obj(GuestAddress(DST + i)).unwrap();
        assert_eq!(jb, ib, "byte at DST+{i:#x}: jit={jb:#x} interp={ib:#x}");
    }
    // Spot-check the expected pattern in the first record.
    assert_eq!(jmem.read_obj::<u8>(GuestAddress(DST)).unwrap(), 0x22, "al");
    assert_eq!(
        jmem.read_obj::<u16>(GuestAddress(DST + 1)).unwrap(),
        0x1122,
        "ax"
    );
    assert_eq!(
        jmem.read_obj::<u32>(GuestAddress(DST + 4)).unwrap(),
        0xEEFF_1122,
        "eax"
    );
    assert_eq!(
        jmem.read_obj::<u64>(GuestAddress(DST + 8)).unwrap(),
        rax_val,
        "rax"
    );
    assert_eq!(
        jmem.read_obj::<u8>(GuestAddress(DST + 16)).unwrap(),
        0x55,
        "imm8"
    );
    assert_eq!(
        jmem.read_obj::<u32>(GuestAddress(DST + 20)).unwrap(),
        0x1234_5678,
        "imm32"
    );
}

/// RIP-relative memory access — the addressing mode kernel code uses for static
/// globals (e.g. the text_poke batch array). The JIT must resolve the absolute
/// guest target at lift time. Must match the interpreter.
#[test]
fn jit_mem_riprel_store_loop_matches_interpreter() {
    const SCRATCH: u64 = 0x20_0000;
    let val = 0x1000u64;

    // mov rax,val ; mov ecx,3
    // loop: mov [rip+disp], rax ; add rax,1 ; dec ecx ; jnz loop ; hlt
    // The mov [rip+disp],rax is `48 89 05 <disp32>`; disp = SCRATCH - next_insn.
    // Layout: mov rax(10) + mov ecx(5) = 15 = loop start; the RIP-rel mov is 7
    // bytes (15..22), so next_insn (guest) = LOAD_ADDR + 22.
    let next_insn = LOAD_ADDR + 22;
    let disp = (SCRATCH as i64 - next_insn as i64) as i32;

    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&[0x48, 0xB8]);
    code.extend_from_slice(&val.to_le_bytes()); // mov rax, val
    code.push(0xB9);
    code.extend_from_slice(&3u32.to_le_bytes()); // mov ecx, 3
    code.extend_from_slice(&[0x48, 0x89, 0x05]); // mov [rip+disp], rax
    code.extend_from_slice(&disp.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x83, 0xC0, 0x01]); // add rax, 1
    code.extend_from_slice(&[0xFF, 0xC9]); // dec ecx
    code.extend_from_slice(&[0x75, 0xF1]); // jnz loop (rel8 = -15)
    code.push(0xF4); // hlt

    let (mut interp, imem) = make_vcpu_mem(&code);
    imem.write_obj(0u64, GuestAddress(SCRATCH)).unwrap();
    run_interp(&mut interp);
    let ir = interp.get_regs().unwrap();
    let iv: u64 = imem.read_obj(GuestAddress(SCRATCH)).unwrap();

    let (mut jit, jmem) = make_vcpu_mem(&code);
    jit.set_jit_mem(true);
    jmem.write_obj(0u64, GuestAddress(SCRATCH)).unwrap();
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "rip-rel loop should JIT"
    );
    run_interp(&mut jit);
    let jr = jit.get_regs().unwrap();
    let jv: u64 = jmem.read_obj(GuestAddress(SCRATCH)).unwrap();

    assert_eq!(jr.rax, ir.rax, "rax");
    assert_eq!(jv, iv, "RIP-relative store target (jit vs interp)");
    assert_eq!(jv, val + 2, "last stored value (val, val+1, val+2)");
}

/// SIB with scale=1 AND a (negative) displacement into an extended register —
/// `mov r9, [rsi + rdx*1 - 16]` — the exact shape the kernel's memmove uses that
/// the JIT verifier flagged. Must match the interpreter.
#[test]
fn jit_mem_sib_scale1_disp_matches_interpreter() {
    const SRC: u64 = 0x20_0000;
    const DST: u64 = 0x21_0000;
    let v = 0x0000_0000_0000_2000u64; // the value at SRC + (rdx - 16)

    // mov rsi,SRC ; mov rdx,0x40 ; mov rdi,DST ; mov ecx,1
    // loop: mov r9,[rsi+rdx*1-16] ; mov [rdi],r9 ; dec ecx ; jnz loop ; hlt
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&[0x48, 0xBE]);
    code.extend_from_slice(&SRC.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xBA]);
    code.extend_from_slice(&0x40u64.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xBF]);
    code.extend_from_slice(&DST.to_le_bytes());
    code.push(0xB9);
    code.extend_from_slice(&1u32.to_le_bytes());
    code.extend_from_slice(&[0x4C, 0x8B, 0x4C, 0x16, 0xF0]); // mov r9, [rsi+rdx*1-16]
    code.extend_from_slice(&[0x4C, 0x89, 0x0F]); // mov [rdi], r9
    code.extend_from_slice(&[0xFF, 0xC9]); // dec ecx
    code.extend_from_slice(&[0x75, 0xF4]); // jnz loop (rel8 = -12)
    code.push(0xF4); // hlt

    // SRC + (rdx=0x40) - 16 = SRC + 0x30.
    let setup = |mem: &Arc<GuestMemoryMmap>| {
        mem.write_obj(v, GuestAddress(SRC + 0x30)).unwrap();
        mem.write_obj(0u64, GuestAddress(DST)).unwrap();
    };

    let (mut interp, imem) = make_vcpu_mem(&code);
    setup(&imem);
    run_interp(&mut interp);
    let iv: u64 = imem.read_obj(GuestAddress(DST)).unwrap();

    let (mut jit, jmem) = make_vcpu_mem(&code);
    jit.set_jit_mem(true);
    setup(&jmem);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "sib-disp loop should JIT"
    );
    run_interp(&mut jit);
    let jv: u64 = jmem.read_obj(GuestAddress(DST)).unwrap();

    assert_eq!(jv, iv, "SIB scale=1 + disp load (jit vs interp)");
    assert_eq!(jv, v, "loaded [rsi+rdx-16] = SRC[0x30]");
}

/// Extended registers (r8-r15) as memory-JIT load destinations AND store
/// sources — the registers the kernel's memmove uses (r8-r11), which none of
/// the other tests exercise. A REX miscoding in spill/reload/deliver would
/// corrupt them. Must match the interpreter byte-for-byte.
#[test]
fn jit_mem_extended_regs_copy_matches_interpreter() {
    const SRC: u64 = 0x20_0000;
    const DST: u64 = 0x21_0000;
    const N: u32 = 4;

    // mov rbx,SRC ; mov rdi,DST ; mov ecx,N
    // loop: mov r9,[rbx] ; mov r11,[rbx+8] ; mov [rdi],r9 ; mov [rdi+8],r11 ;
    //       add rbx,16 ; add rdi,16 ; dec ecx ; jnz loop ; hlt
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&[0x48, 0xBB]);
    code.extend_from_slice(&SRC.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xBF]);
    code.extend_from_slice(&DST.to_le_bytes());
    code.push(0xB9);
    code.extend_from_slice(&N.to_le_bytes());
    // loop body (18 bytes):
    code.extend_from_slice(&[0x4C, 0x8B, 0x0B]); // mov r9, [rbx]
    code.extend_from_slice(&[0x4C, 0x8B, 0x5B, 0x08]); // mov r11, [rbx+8]
    code.extend_from_slice(&[0x4C, 0x89, 0x0F]); // mov [rdi], r9
    code.extend_from_slice(&[0x4C, 0x89, 0x5F, 0x08]); // mov [rdi+8], r11
    code.extend_from_slice(&[0x48, 0x83, 0xC3, 0x10]); // add rbx, 16
    code.extend_from_slice(&[0x48, 0x83, 0xC7, 0x10]); // add rdi, 16
    code.extend_from_slice(&[0xFF, 0xC9]); // dec ecx
    code.extend_from_slice(&[0x75, 0xE6]); // jnz loop (rel8 = -26, body is 24 bytes)
    code.push(0xF4); // hlt

    let seed: [u64; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    let setup = |mem: &Arc<GuestMemoryMmap>| {
        for (i, &x) in seed.iter().enumerate() {
            mem.write_obj(x, GuestAddress(SRC + (i as u64) * 8))
                .unwrap();
            mem.write_obj(0u64, GuestAddress(DST + (i as u64) * 8))
                .unwrap();
        }
    };

    let (mut interp, imem) = make_vcpu_mem(&code);
    setup(&imem);
    run_interp(&mut interp);

    let (mut jit, jmem) = make_vcpu_mem(&code);
    jit.set_jit_mem(true);
    setup(&jmem);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "r8-15 copy should JIT"
    );
    run_interp(&mut jit);

    for i in 0..8u64 {
        let ib: u64 = imem.read_obj(GuestAddress(DST + i * 8)).unwrap();
        let jb: u64 = jmem.read_obj(GuestAddress(DST + i * 8)).unwrap();
        assert_eq!(jb, ib, "DST[{i}] jit vs interp");
        assert_eq!(jb, seed[i as usize], "DST[{i}] == seed (r8-15 copy)");
    }
}

/// OVERLAPPING backwards memmove: dst = src + 8, copied high-to-low so each
/// read precedes the overwrite of that location. This is the read-after-write
/// memmove shape (a load reading a just-stored, overlapping address) that the
/// kernel boot region used and that simple copies don't exercise. The JIT must
/// match the interpreter.
#[test]
fn jit_mem_overlapping_memmove_matches_interpreter() {
    const SRC: u64 = 0x20_0000;
    const N: u32 = 4; // elements

    // rsi = SRC+0x20 ; rdi = SRC+0x28 ; ecx = N
    // loop: mov rax,[rsi-8]; mov [rdi-8],rax; sub rsi,8; sub rdi,8; dec ecx; jnz loop; hlt
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&[0x48, 0xBE]);
    code.extend_from_slice(&(SRC + 0x20).to_le_bytes()); // mov rsi, SRC+0x20
    code.extend_from_slice(&[0x48, 0xBF]);
    code.extend_from_slice(&(SRC + 0x28).to_le_bytes()); // mov rdi, SRC+0x28
    code.push(0xB9);
    code.extend_from_slice(&N.to_le_bytes()); // mov ecx, N
    // loop body (18 bytes):
    code.extend_from_slice(&[0x48, 0x8B, 0x46, 0xF8]); // mov rax, [rsi-8]
    code.extend_from_slice(&[0x48, 0x89, 0x47, 0xF8]); // mov [rdi-8], rax
    code.extend_from_slice(&[0x48, 0x83, 0xEE, 0x08]); // sub rsi, 8
    code.extend_from_slice(&[0x48, 0x83, 0xEF, 0x08]); // sub rdi, 8
    code.extend_from_slice(&[0xFF, 0xC9]); // dec ecx
    code.extend_from_slice(&[0x75, 0xEC]); // jnz loop (rel8 = -20)
    code.push(0xF4); // hlt

    let seed: [u64; 5] = [10, 20, 30, 40, 50];
    let setup = |mem: &Arc<GuestMemoryMmap>| {
        for (i, &x) in seed.iter().enumerate() {
            mem.write_obj(x, GuestAddress(SRC + (i as u64) * 8))
                .unwrap();
        }
    };

    let (mut interp, imem) = make_vcpu_mem(&code);
    setup(&imem);
    run_interp(&mut interp);

    let (mut jit, jmem) = make_vcpu_mem(&code);
    jit.set_jit_mem(true);
    setup(&jmem);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "overlapping memmove should JIT"
    );
    run_interp(&mut jit);

    for i in 0..5u64 {
        let ib: u64 = imem.read_obj(GuestAddress(SRC + i * 8)).unwrap();
        let jb: u64 = jmem.read_obj(GuestAddress(SRC + i * 8)).unwrap();
        assert_eq!(jb, ib, "SRC[{i}] jit vs interp (overlapping memmove)");
    }
    // Expected memmove(SRC+8, SRC, 32): [10, 10, 20, 30, 40].
    assert_eq!(
        jmem.read_obj::<u64>(GuestAddress(SRC + 8)).unwrap(),
        10,
        "moved[1]"
    );
    assert_eq!(
        jmem.read_obj::<u64>(GuestAddress(SRC + 32)).unwrap(),
        40,
        "moved[4]"
    );
}

/// Exact reconstruction of the kernel-boot memmove block-0: a 32-byte backwards
/// copy loop whose counter is `sub rdx,32` (CF/`jae`), with EIGHT memory ops
/// (4 loads + 4 stores via r8-r11) between the flag-set and the branch. This is
/// the region the JIT verifier flagged; the flags must survive all 8 mem ops so
/// the CF-based loop count matches the interpreter.
#[test]
fn jit_mem_boot_memcpy_block0_matches_interpreter() {
    const SRC: u64 = 0x20_0000;
    const DST: u64 = 0x21_0000;
    const BYTES: u64 = 0x80; // rdx start; loop copies 32 at a time

    // mov rsi,SRC+BYTES ; mov rdi,DST+BYTES ; mov rdx,BYTES
    // loop: sub rdx,32 ; mov r11,[rsi-8]; r10,[rsi-16]; r9,[rsi-24]; r8,[rsi-32];
    //       lea rsi,[rsi-32]; mov [rdi-8],r11; [rdi-16],r10; [rdi-24],r9; [rdi-32],r8;
    //       lea rdi,[rdi-32]; jae loop ; hlt
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&[0x48, 0xBE]);
    code.extend_from_slice(&(SRC + BYTES).to_le_bytes());
    code.extend_from_slice(&[0x48, 0xBF]);
    code.extend_from_slice(&(DST + BYTES).to_le_bytes());
    code.extend_from_slice(&[0x48, 0xBA]);
    code.extend_from_slice(&BYTES.to_le_bytes());
    // block 0 (44 bytes), bytes copied verbatim from the boot region dump:
    code.extend_from_slice(&[0x48, 0x83, 0xEA, 0x20]); // sub rdx, 32
    code.extend_from_slice(&[0x4C, 0x8B, 0x5E, 0xF8]); // mov r11, [rsi-8]
    code.extend_from_slice(&[0x4C, 0x8B, 0x56, 0xF0]); // mov r10, [rsi-16]
    code.extend_from_slice(&[0x4C, 0x8B, 0x4E, 0xE8]); // mov r9, [rsi-24]
    code.extend_from_slice(&[0x4C, 0x8B, 0x46, 0xE0]); // mov r8, [rsi-32]
    code.extend_from_slice(&[0x48, 0x8D, 0x76, 0xE0]); // lea rsi, [rsi-32]
    code.extend_from_slice(&[0x4C, 0x89, 0x5F, 0xF8]); // mov [rdi-8], r11
    code.extend_from_slice(&[0x4C, 0x89, 0x57, 0xF0]); // mov [rdi-16], r10
    code.extend_from_slice(&[0x4C, 0x89, 0x4F, 0xE8]); // mov [rdi-24], r9
    code.extend_from_slice(&[0x4C, 0x89, 0x47, 0xE0]); // mov [rdi-32], r8
    code.extend_from_slice(&[0x48, 0x8D, 0x7F, 0xE0]); // lea rdi, [rdi-32]
    code.extend_from_slice(&[0x73, 0xD2]); // jae loop (rel8 = -46)
    code.push(0xF4); // hlt

    let setup = |mem: &Arc<GuestMemoryMmap>| {
        for i in 0..(BYTES / 8) {
            mem.write_obj(0x1000 + i, GuestAddress(SRC + i * 8))
                .unwrap();
            mem.write_obj(0u64, GuestAddress(DST + i * 8)).unwrap();
        }
    };

    let (mut interp, imem) = make_vcpu_mem(&code);
    setup(&imem);
    run_interp(&mut interp);
    let ir = interp.get_regs().unwrap();

    let (mut jit, jmem) = make_vcpu_mem(&code);
    jit.set_jit_mem(true);
    setup(&jmem);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "block0 memcpy should JIT"
    );
    run_interp(&mut jit);
    let jr = jit.get_regs().unwrap();

    assert_eq!(jr.rdx, ir.rdx, "rdx (CF-based loop counter)");
    assert_eq!(jr.rsi, ir.rsi, "rsi");
    assert_eq!(jr.rdi, ir.rdi, "rdi");
    for i in 0..(BYTES / 8) {
        let ib: u64 = imem.read_obj(GuestAddress(DST + i * 8)).unwrap();
        let jb: u64 = jmem.read_obj(GuestAddress(DST + i * 8)).unwrap();
        assert_eq!(jb, ib, "DST[{i}] jit vs interp");
    }
}

/// Boot block-0 with the EXACT overlap from the failing kernel region:
/// dst = src + 24, length rdx = 0x130 (non-32-multiple). A backwards 32-byte
/// copy within one overlapping buffer — the read-after-write geometry the
/// non-overlapping block-0 test doesn't reach. JIT must match the interpreter.
#[test]
fn jit_mem_boot_memmove_overlap24_matches_interpreter() {
    const BASE: u64 = 0x20_0000;
    const LEN: u64 = 0x130;
    let rsi0 = BASE + 0x200; // src end (high), copy proceeds downward
    let rdi0 = rsi0 + 0x18; // dst end = src + 24 (overlap)

    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&[0x48, 0xBE]);
    code.extend_from_slice(&rsi0.to_le_bytes()); // mov rsi, rsi0
    code.extend_from_slice(&[0x48, 0xBF]);
    code.extend_from_slice(&rdi0.to_le_bytes()); // mov rdi, rdi0
    code.extend_from_slice(&[0x48, 0xBA]);
    code.extend_from_slice(&LEN.to_le_bytes()); // mov rdx, LEN
    code.extend_from_slice(&[0x48, 0x83, 0xEA, 0x20]); // sub rdx, 32
    code.extend_from_slice(&[0x4C, 0x8B, 0x5E, 0xF8]); // mov r11, [rsi-8]
    code.extend_from_slice(&[0x4C, 0x8B, 0x56, 0xF0]); // mov r10, [rsi-16]
    code.extend_from_slice(&[0x4C, 0x8B, 0x4E, 0xE8]); // mov r9, [rsi-24]
    code.extend_from_slice(&[0x4C, 0x8B, 0x46, 0xE0]); // mov r8, [rsi-32]
    code.extend_from_slice(&[0x48, 0x8D, 0x76, 0xE0]); // lea rsi, [rsi-32]
    code.extend_from_slice(&[0x4C, 0x89, 0x5F, 0xF8]); // mov [rdi-8], r11
    code.extend_from_slice(&[0x4C, 0x89, 0x57, 0xF0]); // mov [rdi-16], r10
    code.extend_from_slice(&[0x4C, 0x89, 0x4F, 0xE8]); // mov [rdi-24], r9
    code.extend_from_slice(&[0x4C, 0x89, 0x47, 0xE0]); // mov [rdi-32], r8
    code.extend_from_slice(&[0x48, 0x8D, 0x7F, 0xE0]); // lea rdi, [rdi-32]
    code.extend_from_slice(&[0x73, 0xD2]); // jae loop
    code.push(0xF4); // hlt

    let setup = |mem: &Arc<GuestMemoryMmap>| {
        for i in 0..0x60u64 {
            mem.write_obj(0x100000 + i, GuestAddress(BASE + i * 8))
                .unwrap();
        }
    };

    let (mut interp, imem) = make_vcpu_mem(&code);
    setup(&imem);
    run_interp(&mut interp);

    let (mut jit, jmem) = make_vcpu_mem(&code);
    jit.set_jit_mem(true);
    setup(&jmem);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "overlap-24 memmove should JIT"
    );
    run_interp(&mut jit);

    for i in 0..0x60u64 {
        let ib: u64 = imem.read_obj(GuestAddress(BASE + i * 8)).unwrap();
        let jb: u64 = jmem.read_obj(GuestAddress(BASE + i * 8)).unwrap();
        assert_eq!(jb, ib, "buf[{i}] jit vs interp (overlap-24 memmove)");
    }
}

/// A long byte-copy loop (`mov al,[rsi]; mov [rdi],al; inc rdi; inc rsi; dec rcx;
/// jne`) — the kernel `memcpy` byte tail at region 0x8214929f. Each iteration is
/// TWO mem-helper calls; checks the host stack stays balanced over many
/// iterations (RSP drift would corrupt the host stack → crash) and that RSP +
/// the copied bytes match the interpreter.
#[test]
fn jit_mem_byte_copy_loop_long() {
    const SRC: u64 = 0x20_0000;
    const DST: u64 = 0x40_0000;
    const N: u64 = 0x1000;
    let code: &[u8] = &[
        0x8a, 0x06, // mov al, [rsi]
        0x88, 0x07, // mov [rdi], al
        0x48, 0xff, 0xc7, // inc rdi
        0x48, 0xff, 0xc6, // inc rsi
        0x48, 0xff, 0xc9, // dec rcx
        0x75, 0xf3, // jne loop (-13)
        0xf4, // hlt
    ];
    let setup = |v: &mut X86_64Vcpu, mem: &Arc<GuestMemoryMmap>| {
        for i in 0..N {
            mem.write_obj(
                (i as u8).wrapping_mul(7).wrapping_add(3),
                GuestAddress(SRC + i),
            )
            .unwrap();
        }
        let mut r = v.get_regs().unwrap();
        r.rsi = SRC;
        r.rdi = DST;
        r.rcx = N;
        v.set_regs(&r).unwrap();
    };

    let (mut interp, im) = make_vcpu_mem(code);
    setup(&mut interp, &im);
    run_interp(&mut interp);

    let (mut jit, jm) = make_vcpu_mem(code);
    setup(&mut jit, &jm);
    jit.set_jit_mem(true);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "region should JIT"
    );
    run_interp(&mut jit);

    assert_eq!(
        jit.get_regs().unwrap().rsp,
        interp.get_regs().unwrap().rsp,
        "RSP drift"
    );
    assert_eq!(jit.get_regs().unwrap().rcx, 0, "rcx");
    for i in 0..N {
        let ib: u8 = im.read_obj(GuestAddress(DST + i)).unwrap();
        let jb: u8 = jm.read_obj(GuestAddress(DST + i)).unwrap();
        assert_eq!(jb, ib, "DST[{i}]");
    }
}

// FS/GS segment-relative memory operands (the `64`/`65` prefixes) must JIT
// CORRECTLY: the effective address is `segment_base + base + index*scale + disp`,
// where the base comes from the FS/GS descriptor / IA32_FS_BASE/GS_BASE MSR
// (`sregs.fs.base`/`sregs.gs.base`), lifted as `Address::SegmentRel`. These are
// the kernel's per-CPU (`gs:`) and TLS (`fs:`) accesses. Each test uses a
// NON-ZERO segment base, places the correct value at `seg_base+addr` AND a
// distinct SENTINEL at the un-segmented `addr`, so a JIT that dropped the
// segment base would read the sentinel and diverge from the interpreter.
fn seg_jit_vs_interp(
    code: &[u8],
    fs_base: u64,
    gs_base: u64,
    setup: impl Fn(&mut X86_64Vcpu, &Arc<GuestMemoryMmap>),
) -> (
    X86_64Vcpu,
    Arc<GuestMemoryMmap>,
    X86_64Vcpu,
    Arc<GuestMemoryMmap>,
) {
    let prep = |v: &mut X86_64Vcpu, m: &Arc<GuestMemoryMmap>| {
        let mut s = v.get_sregs().unwrap();
        s.fs.base = fs_base;
        s.gs.base = gs_base;
        v.set_sregs(&s).unwrap();
        setup(v, m);
    };
    let (mut interp, im) = make_vcpu_mem(code);
    prep(&mut interp, &im);
    run_interp(&mut interp);

    let (mut jit, jm) = make_vcpu_mem(code);
    prep(&mut jit, &jm);
    jit.set_jit_mem(true);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "FS/GS-relative region MUST now JIT (Address::SegmentRel)"
    );
    run_interp(&mut jit);
    (interp, im, jit, jm)
}

const GSB: u64 = 0x50_0000;
const FSB: u64 = 0x60_0000;

// Each test wraps the segment op in a 1-iteration loop (`dec <ctr>; jne head`)
// so the region's entry block ends in a back-edge (not a frontier) and the JIT
// actually compiles + runs the op. The counter is a register the op doesn't use.

/// `mov rax, gs:[rbx+8]` — base + disp, GS base added.
#[test]
fn jit_mem_gs_relative_base_disp() {
    // loop: mov rax, gs:[rbx+8]; dec rcx; jne loop; hlt
    let code: &[u8] = &[
        0x65, 0x48, 0x8b, 0x43, 0x08, 0x48, 0xff, 0xc9, 0x75, 0xf6, 0xf4,
    ];
    let setup = |v: &mut X86_64Vcpu, m: &Arc<GuestMemoryMmap>| {
        m.write_obj(0xCAFEu64, GuestAddress(GSB + 0x1008)).unwrap(); // gs.base+rbx+8
        m.write_obj(0xBADu64, GuestAddress(0x1008)).unwrap(); // sentinel (no gs base)
        let mut r = v.get_regs().unwrap();
        r.rbx = 0x1000;
        r.rcx = 1;
        v.set_regs(&r).unwrap();
    };
    let (interp, _im, jit, _jm) = seg_jit_vs_interp(code, 0, GSB, setup);
    assert_eq!(
        jit.get_regs().unwrap().rax,
        interp.get_regs().unwrap().rax,
        "rax jit vs interp"
    );
    assert_eq!(
        jit.get_regs().unwrap().rax,
        0xCAFE,
        "must read [gs.base+rbx+8], not sentinel"
    );
}

/// `mov rax, gs:[0x1234]` — disp-only (the kernel `this_cpu` per-CPU pattern).
#[test]
fn jit_mem_gs_relative_disp_only() {
    // loop: mov rax, gs:[0x1234]; dec rcx; jne loop; hlt
    let code: &[u8] = &[
        0x65, 0x48, 0x8b, 0x04, 0x25, 0x34, 0x12, 0x00, 0x00, 0x48, 0xff, 0xc9, 0x75, 0xf2, 0xf4,
    ];
    let setup = |v: &mut X86_64Vcpu, m: &Arc<GuestMemoryMmap>| {
        m.write_obj(0xDEADu64, GuestAddress(GSB + 0x1234)).unwrap();
        m.write_obj(0xBADu64, GuestAddress(0x1234)).unwrap();
        let mut r = v.get_regs().unwrap();
        r.rcx = 1;
        v.set_regs(&r).unwrap();
    };
    let (interp, _im, jit, _jm) = seg_jit_vs_interp(code, 0, GSB, setup);
    assert_eq!(
        jit.get_regs().unwrap().rax,
        interp.get_regs().unwrap().rax,
        "rax jit vs interp"
    );
    assert_eq!(
        jit.get_regs().unwrap().rax,
        0xDEAD,
        "must read [gs.base+0x1234]"
    );
}

/// `mov rax, fs:[rbx]` — FS base added (TLS).
#[test]
fn jit_mem_fs_relative() {
    // loop: mov rax, fs:[rbx]; dec rcx; jne loop; hlt
    let code: &[u8] = &[0x64, 0x48, 0x8b, 0x03, 0x48, 0xff, 0xc9, 0x75, 0xf7, 0xf4];
    let setup = |v: &mut X86_64Vcpu, m: &Arc<GuestMemoryMmap>| {
        m.write_obj(0xF00Du64, GuestAddress(FSB + 0x800)).unwrap();
        m.write_obj(0xBADu64, GuestAddress(0x800)).unwrap();
        let mut r = v.get_regs().unwrap();
        r.rbx = 0x800;
        r.rcx = 1;
        v.set_regs(&r).unwrap();
    };
    let (interp, _im, jit, _jm) = seg_jit_vs_interp(code, FSB, 0, setup);
    assert_eq!(
        jit.get_regs().unwrap().rax,
        interp.get_regs().unwrap().rax,
        "rax jit vs interp"
    );
    assert_eq!(
        jit.get_regs().unwrap().rax,
        0xF00D,
        "must read [fs.base+rbx]"
    );
}

/// `mov gs:[rbx], rax` — STORE to a GS-relative address.
#[test]
fn jit_mem_gs_relative_store() {
    // loop: mov gs:[rbx], rax; dec rcx; jne loop; hlt
    let code: &[u8] = &[0x65, 0x48, 0x89, 0x03, 0x48, 0xff, 0xc9, 0x75, 0xf7, 0xf4];
    let setup = |v: &mut X86_64Vcpu, m: &Arc<GuestMemoryMmap>| {
        m.write_obj(0u64, GuestAddress(GSB + 0x900)).unwrap();
        m.write_obj(0u64, GuestAddress(0x900)).unwrap();
        let mut r = v.get_regs().unwrap();
        r.rbx = 0x900;
        r.rcx = 1;
        r.rax = 0x1234_5678_9ABC_DEF0;
        v.set_regs(&r).unwrap();
    };
    let (_interp, _im, _jit, jm) = seg_jit_vs_interp(code, 0, GSB, setup);
    let stored: u64 = jm.read_obj(GuestAddress(GSB + 0x900)).unwrap();
    assert_eq!(
        stored, 0x1234_5678_9ABC_DEF0,
        "store must hit [gs.base+rbx]"
    );
    let sentinel: u64 = jm.read_obj(GuestAddress(0x900)).unwrap();
    assert_eq!(sentinel, 0, "store must NOT hit the un-segmented address");
}

/// `mov rax, gs:[rbx+rcx*8]` — base + index*scale + GS base (full SIB form).
/// Uses RDX as the loop counter (RCX is the index).
#[test]
fn jit_mem_gs_relative_index_scale() {
    // loop: mov rax, gs:[rbx+rcx*8]; dec rdx; jne loop; hlt
    let code: &[u8] = &[
        0x65, 0x48, 0x8b, 0x04, 0xcb, 0x48, 0xff, 0xca, 0x75, 0xf6, 0xf4,
    ];
    let setup = |v: &mut X86_64Vcpu, m: &Arc<GuestMemoryMmap>| {
        // rbx=0x1000, rcx=3 → gs.base + 0x1000 + 3*8 = gs.base + 0x1018
        m.write_obj(0xBEEFu64, GuestAddress(GSB + 0x1018)).unwrap();
        m.write_obj(0xBADu64, GuestAddress(0x1018)).unwrap();
        let mut r = v.get_regs().unwrap();
        r.rbx = 0x1000;
        r.rcx = 3;
        r.rdx = 1;
        v.set_regs(&r).unwrap();
    };
    let (interp, _im, jit, _jm) = seg_jit_vs_interp(code, 0, GSB, setup);
    assert_eq!(
        jit.get_regs().unwrap().rax,
        interp.get_regs().unwrap().rax,
        "rax jit vs interp"
    );
    assert_eq!(
        jit.get_regs().unwrap().rax,
        0xBEEF,
        "must read [gs.base+rbx+rcx*8]"
    );
}

/// `mov al, gs:[rbx]` (B1) — a partial-register write: x86 `mov r8, r/m8`
/// preserves the upper 56 bits of RAX, it does NOT zero-extend. The kernel's
/// per-CPU byte reads rely on this; a JIT that zero-extended would corrupt the
/// upper bits (the exact boot divergence `rax: interp=0x80010000 jit=0x0`).
#[test]
fn jit_mem_gs_relative_byte_partial_write() {
    // loop: mov al, gs:[rbx]; dec rcx; jne loop; hlt
    let code: &[u8] = &[0x65, 0x8a, 0x03, 0x48, 0xff, 0xc9, 0x75, 0xf8, 0xf4];
    let setup = |v: &mut X86_64Vcpu, m: &Arc<GuestMemoryMmap>| {
        m.write_obj(0x42u8, GuestAddress(GSB + 0x700)).unwrap();
        let mut r = v.get_regs().unwrap();
        r.rbx = 0x700;
        r.rcx = 1;
        r.rax = 0xDEAD_BEEF_0000_0000;
        v.set_regs(&r).unwrap();
    };
    let (interp, _im, jit, _jm) = seg_jit_vs_interp(code, 0, GSB, setup);
    assert_eq!(
        jit.get_regs().unwrap().rax,
        interp.get_regs().unwrap().rax,
        "rax jit vs interp"
    );
    assert_eq!(
        jit.get_regs().unwrap().rax,
        0xDEAD_BEEF_0000_0042,
        "mov al must preserve the upper 56 bits of RAX (partial write)"
    );
}

/// `mov ax, gs:[rbx]` (B2) — partial-register write preserving the upper 48 bits.
#[test]
fn jit_mem_gs_relative_word_partial_write() {
    // loop: mov ax, gs:[rbx]; dec rcx; jne loop; hlt  (65=GS, 66=opsize)
    let code: &[u8] = &[0x65, 0x66, 0x8b, 0x03, 0x48, 0xff, 0xc9, 0x75, 0xf7, 0xf4];
    let setup = |v: &mut X86_64Vcpu, m: &Arc<GuestMemoryMmap>| {
        m.write_obj(0x1234u16, GuestAddress(GSB + 0x780)).unwrap();
        let mut r = v.get_regs().unwrap();
        r.rbx = 0x780;
        r.rcx = 1;
        r.rax = 0xFFFF_FFFF_FFFF_FFFF;
        v.set_regs(&r).unwrap();
    };
    let (interp, _im, jit, _jm) = seg_jit_vs_interp(code, 0, GSB, setup);
    assert_eq!(
        jit.get_regs().unwrap().rax,
        interp.get_regs().unwrap().rax,
        "rax jit vs interp"
    );
    assert_eq!(
        jit.get_regs().unwrap().rax,
        0xFFFF_FFFF_FFFF_1234,
        "mov ax must preserve the upper 48 bits of RAX (partial write)"
    );
}

/// `mov eax, gs:[rbx]` (B4) — a 32-bit write ZERO-EXTENDS to 64 bits (clears the
/// upper 32 of RAX), unlike B1/B2. Confirms the deliver path's width semantics.
#[test]
fn jit_mem_gs_relative_dword_zero_extend() {
    // loop: mov eax, gs:[rbx]; dec rcx; jne loop; hlt
    let code: &[u8] = &[0x65, 0x8b, 0x03, 0x48, 0xff, 0xc9, 0x75, 0xf8, 0xf4];
    let setup = |v: &mut X86_64Vcpu, m: &Arc<GuestMemoryMmap>| {
        m.write_obj(0x1234_5678u32, GuestAddress(GSB + 0x680))
            .unwrap();
        let mut r = v.get_regs().unwrap();
        r.rbx = 0x680;
        r.rcx = 1;
        r.rax = 0xFFFF_FFFF_FFFF_FFFF;
        v.set_regs(&r).unwrap();
    };
    let (interp, _im, jit, _jm) = seg_jit_vs_interp(code, 0, GSB, setup);
    assert_eq!(
        jit.get_regs().unwrap().rax,
        interp.get_regs().unwrap().rax,
        "rax jit vs interp"
    );
    assert_eq!(
        jit.get_regs().unwrap().rax,
        0x0000_0000_1234_5678,
        "mov eax must zero-extend (clear upper 32 of RAX)"
    );
}

/// `mov rax, gs:[rbx-8]` — negative displacement, GS base added.
#[test]
fn jit_mem_gs_relative_negative_disp() {
    // loop: mov rax, gs:[rbx-8]; dec rcx; jne loop; hlt
    let code: &[u8] = &[
        0x65, 0x48, 0x8b, 0x43, 0xf8, 0x48, 0xff, 0xc9, 0x75, 0xf6, 0xf4,
    ];
    let setup = |v: &mut X86_64Vcpu, m: &Arc<GuestMemoryMmap>| {
        m.write_obj(0xC0DEu64, GuestAddress(GSB + 0x1000 - 8))
            .unwrap(); // gs.base + rbx - 8
        let mut r = v.get_regs().unwrap();
        r.rbx = 0x1000;
        r.rcx = 1;
        v.set_regs(&r).unwrap();
    };
    let (interp, _im, jit, _jm) = seg_jit_vs_interp(code, 0, GSB, setup);
    assert_eq!(
        jit.get_regs().unwrap().rax,
        interp.get_regs().unwrap().rax,
        "rax jit vs interp"
    );
    assert_eq!(
        jit.get_regs().unwrap().rax,
        0xC0DE,
        "must read [gs.base+rbx-8]"
    );
}

/// `mov fs:[rbx+rcx*4+0x10], rax` — FS store with base + index*scale + disp.
#[test]
fn jit_mem_fs_relative_store_index_disp() {
    // loop: mov fs:[rbx+rcx*4+0x10], rax; dec rdx; jne loop; hlt
    let code: &[u8] = &[
        0x64, 0x48, 0x89, 0x44, 0x8b, 0x10, 0x48, 0xff, 0xca, 0x75, 0xf5, 0xf4,
    ];
    let setup = |v: &mut X86_64Vcpu, m: &Arc<GuestMemoryMmap>| {
        // rbx=0x100, rcx=2 → fs.base + 0x100 + 2*4 + 0x10 = fs.base + 0x118
        m.write_obj(0u64, GuestAddress(FSB + 0x118)).unwrap();
        let mut r = v.get_regs().unwrap();
        r.rbx = 0x100;
        r.rcx = 2;
        r.rdx = 1;
        r.rax = 0xABCD_1234_5678_9ABC;
        v.set_regs(&r).unwrap();
    };
    let (_interp, _im, _jit, jm) = seg_jit_vs_interp(code, FSB, 0, setup);
    let stored: u64 = jm.read_obj(GuestAddress(FSB + 0x118)).unwrap();
    assert_eq!(
        stored, 0xABCD_1234_5678_9ABC,
        "store must hit [fs.base+rbx+rcx*4+0x10]"
    );
}

/// `lea rax, fs:[rbx]` — LEA computes the OFFSET and must IGNORE the segment
/// override: it yields rbx, NOT fs.base+rbx, even with a non-zero fs.base.
/// (End-to-end guard for the LEA-adds-segment-base bug the audit found.)
#[test]
fn jit_mem_lea_ignores_segment() {
    // loop: lea rax, fs:[rbx]; dec rcx; jne loop; hlt
    let code: &[u8] = &[0x64, 0x48, 0x8d, 0x03, 0x48, 0xff, 0xc9, 0x75, 0xf7, 0xf4];
    let setup = |v: &mut X86_64Vcpu, m: &Arc<GuestMemoryMmap>| {
        let _ = m;
        let mut r = v.get_regs().unwrap();
        r.rbx = 0x1234;
        r.rcx = 1;
        v.set_regs(&r).unwrap();
    };
    let (interp, _im, jit, _jm) = seg_jit_vs_interp(code, FSB, 0, setup);
    assert_eq!(
        jit.get_regs().unwrap().rax,
        interp.get_regs().unwrap().rax,
        "rax jit vs interp"
    );
    assert_eq!(
        jit.get_regs().unwrap().rax,
        0x1234,
        "LEA must ignore fs.base (yield the offset rbx)"
    );
}

/// `mov gs:[rbx+rcx*4], rax` — GS STORE with base + index*scale.
#[test]
fn jit_mem_gs_relative_store_index_scale() {
    // loop: mov gs:[rbx+rcx*4], rax; dec rdx; jne loop; hlt  (rdx counter; rcx is index)
    let code: &[u8] = &[
        0x65, 0x48, 0x89, 0x04, 0x8b, 0x48, 0xff, 0xca, 0x75, 0xf6, 0xf4,
    ];
    let setup = |v: &mut X86_64Vcpu, m: &Arc<GuestMemoryMmap>| {
        m.write_obj(0u64, GuestAddress(GSB + 0x1008)).unwrap(); // gs.base + 0x1000 + 2*4
        let mut r = v.get_regs().unwrap();
        r.rbx = 0x1000;
        r.rcx = 2;
        r.rdx = 1;
        r.rax = 0xCAFE_BABE_DEAD_BEEF;
        v.set_regs(&r).unwrap();
    };
    let (_interp, _im, _jit, jm) = seg_jit_vs_interp(code, 0, GSB, setup);
    let stored: u64 = jm.read_obj(GuestAddress(GSB + 0x1008)).unwrap();
    assert_eq!(
        stored, 0xCAFE_BABE_DEAD_BEEF,
        "store must hit [gs.base+rbx+rcx*4]"
    );
}

/// `movzx ecx, dil` (REX-prefixed `40 0f b6 cf`) wedged BETWEEN two loads, as in
/// kernel region 0x82149bd0. The lifter must not drop the movzx — if it does,
/// rcx keeps a stale value and the dependent indexed load reads a wrong address.
#[test]
fn jit_mem_movzx_dil_between_loads() {
    const PTR: u64 = 0x20_0000;
    const DATA: u64 = 0x21_0000;
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&[0x48, 0x8b, 0x02]); // mov rax, [rdx]
    code.extend_from_slice(&[0x40, 0x0f, 0xb6, 0xcf]); // movzx ecx, dil
    code.extend_from_slice(&[0x8b, 0x04, 0x88]); // mov eax, [rax+rcx*4]
    code.extend_from_slice(&[0x48, 0xff, 0xce]); // dec rsi
    code.extend_from_slice(&[0x75, 0xf1]); // jne loop (-15)
    code.push(0xf4); // hlt

    let setup = |v: &mut X86_64Vcpu, mem: &Arc<GuestMemoryMmap>| {
        mem.write_obj(DATA, GuestAddress(PTR)).unwrap();
        mem.write_obj(0x8580u32, GuestAddress(DATA + 4)).unwrap(); // [DATA + dil*4], dil=1
        let mut r = v.get_regs().unwrap();
        r.rdx = PTR;
        r.rdi = 1; // dil = 1 → rcx should become 1
        r.rcx = 0x3c; // stale value that must be overwritten by the movzx
        r.rsi = 1;
        v.set_regs(&r).unwrap();
    };

    let (mut interp, im) = make_vcpu_mem(&code);
    setup(&mut interp, &im);
    run_interp(&mut interp);

    let (mut jit, jm) = make_vcpu_mem(&code);
    setup(&mut jit, &jm);
    jit.set_jit_mem(true);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "region should JIT"
    );
    run_interp(&mut jit);

    assert_eq!(interp.get_regs().unwrap().rcx, 1, "interp rcx (movzx dil)");
    assert_eq!(
        jit.get_regs().unwrap().rcx,
        1,
        "jit rcx (movzx must not be dropped)"
    );
    assert_eq!(interp.get_regs().unwrap().rax, 0x8580, "interp rax");
    assert_eq!(
        jit.get_regs().unwrap().rax,
        0x8580,
        "jit rax (depends on rcx=1)"
    );
}

/// BaseIndexScale loads with scale=4 and 32-bit (B4) width — the shape in the
/// kernel region 0x82149bd0 that my scale=1/scale=8 tests didn't cover. Also a
/// dependent pair (load a pointer, then index off it), which 0x82149bd0 uses.
#[test]
fn jit_mem_baseindexscale_scale4_b4_and_dependent() {
    const PTR: u64 = 0x20_0000; // holds a pointer
    const DATA: u64 = 0x21_0000; // pointed-to table

    // mov rax,[rdx]; mov eax,[rax+rcx*4]; dec rsi; jne loop; hlt
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&[0x48, 0x8b, 0x02]); // mov rax, [rdx]
    code.extend_from_slice(&[0x8b, 0x04, 0x88]); // mov eax, [rax+rcx*4]  (B4, scale 4)
    code.extend_from_slice(&[0x48, 0xff, 0xce]); // dec rsi
    code.extend_from_slice(&[0x75, 0xf6]); // jne loop (-10)
    code.push(0xf4); // hlt

    let setup = |v: &mut X86_64Vcpu, mem: &Arc<GuestMemoryMmap>| {
        mem.write_obj(DATA, GuestAddress(PTR)).unwrap(); // [rdx] = DATA pointer
        mem.write_obj(0x8580u32, GuestAddress(DATA + 8)).unwrap(); // [DATA + rcx*4], rcx=2
        let mut r = v.get_regs().unwrap();
        r.rdx = PTR;
        r.rcx = 2;
        r.rsi = 1; // one iteration
        r.rax = 0xDEAD;
        v.set_regs(&r).unwrap();
    };

    let (mut interp, im) = make_vcpu_mem(&code);
    setup(&mut interp, &im);
    run_interp(&mut interp);

    let (mut jit, jm) = make_vcpu_mem(&code);
    setup(&mut jit, &jm);
    jit.set_jit_mem(true);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "region should JIT"
    );
    run_interp(&mut jit);

    assert_eq!(interp.get_regs().unwrap().rax, 0x8580, "interp rax");
    assert_eq!(
        jit.get_regs().unwrap().rax,
        0x8580,
        "jit rax (scale4/B4 dependent load)"
    );
}

/// Reconstruction of kernel region 0x8173fd10 (a memchr/scan helper) which the
/// verifier flagged with a flags divergence: the path reaches a frontier exit
/// right after `xor ecx,ecx`, so the JIT must materialize the xor's flags
/// (ZF+PF = 0x44) at the exit, not the preceding cmp's flags. Checks the
/// flag state at the frontier exit, where the bug lives (later ops overwrite it).
#[test]
fn jit_mem_xor_flags_at_frontier_exit() {
    const BUF: u64 = 0x30_0000;
    // Verbatim region bytes from the boot at 0xffffffff8173fd10.
    let region: &[u8] = &[
        0x48, 0x85, 0xf6, 0x74, 0x0d, 0x48, 0x8b, 0x07, 0x48, 0x83, 0xf8, 0xff, 0x74, 0x0a, 0x31,
        0xc9, 0xeb, 0x22, 0x31, 0xf6, 0x48, 0x89, 0xf0, 0xc3, 0x48, 0x83, 0xc7, 0x08, 0x31, 0xc9,
        0x48, 0x83, 0xc1, 0x40, 0x48, 0x39, 0xf1, 0x73, 0xed, 0x48, 0x8b, 0x07, 0x48, 0x83, 0xc7,
        0x08, 0x48, 0x83, 0xf8, 0xff, 0x74, 0xea, 0x48, 0xf7, 0xd0, 0xf3, 0x48, 0x0f, 0xbc, 0xc0,
        0x48, 0x01, 0xc8, 0x48, 0x39, 0xf0, 0x48, 0x0f, 0x43, 0xc6, 0xc3,
    ];
    const MASK: u64 = 0x0ED5;
    const STACK: u64 = 0x18_0000;
    const RET_HLT: u64 = LOAD_ADDR + 0x200;

    let setup = |v: &mut X86_64Vcpu, mem: &Arc<GuestMemoryMmap>| {
        mem.write_obj(0x12345u64, GuestAddress(BUF)).unwrap(); // [rdi] != -1 → xor path
        mem.write_obj(0xF4u8, GuestAddress(RET_HLT)).unwrap(); // hlt at the ret target
        mem.write_obj(RET_HLT, GuestAddress(STACK + 8)).unwrap(); // [rsp+8] (after add rsp,8; ret)
        let mut r = v.get_regs().unwrap();
        r.rsi = 0x400;
        r.rdi = BUF;
        r.rbx = BUF;
        r.rax = 0x7e;
        r.rcx = 0x7e;
        r.rdx = 0x40;
        r.rsp = STACK;
        v.set_regs(&r).unwrap();
    };

    // End-to-end: the JIT runs block0→2→4 then hands off at the block-5 frontier;
    // the interpreter resumes into block 5, which overwrites every flag before
    // reading it. So although the JIT's flags differ from the interpreter's AT
    // the frontier (the benign dead-flag artifact: xor's ZF+PF vs the eliminated
    // result), the FINAL architectural state — registers AND flags — must match.
    let (mut interp, _im) = make_vcpu_mem(region);
    setup(&mut interp, &_im);
    run_interp(&mut interp);

    let (mut jit, jm) = make_vcpu_mem(region);
    setup(&mut jit, &jm);
    jit.set_jit_mem(true);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "region should JIT"
    );
    run_interp(&mut jit);

    let ir = interp.get_regs().unwrap();
    let jr = jit.get_regs().unwrap();
    assert_eq!(jr.rax, ir.rax, "final rax");
    assert_eq!(jr.rcx, ir.rcx, "final rcx");
    assert_eq!(jr.rdx, ir.rdx, "final rdx");
    assert_eq!(jr.rsi, ir.rsi, "final rsi");
    assert_eq!(
        jr.rflags & MASK,
        ir.rflags & MASK,
        "final flags must match once block 5 overwrites the dead frontier flags"
    );
}

/// Minimal reproduction of the kernel-memmove tail bug: two `[rsi+rdx*1+disp]`
/// loads differing ONLY in displacement (-16, -8) into r9/r8, then two matching
/// stores. The JIT must lift BOTH loads — dropping `mov r8,[rsi+rdx-8]` while
/// keeping `mov [rdi+rdx-8],r8` stores a stale r8 (the boot memmove corruption).
#[test]
fn jit_mem_two_baseindexscale_loads_distinct_disp() {
    const SRC: u64 = 0x20_0000;
    const DST: u64 = 0x21_0000;
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&[0x4c, 0x8b, 0x4c, 0x16, 0xf0]); // mov r9, [rsi+rdx*1-16]
    code.extend_from_slice(&[0x4c, 0x8b, 0x44, 0x16, 0xf8]); // mov r8, [rsi+rdx*1-8]
    code.extend_from_slice(&[0x4c, 0x89, 0x4c, 0x17, 0xf0]); // mov [rdi+rdx*1-16], r9
    code.extend_from_slice(&[0x4c, 0x89, 0x44, 0x17, 0xf8]); // mov [rdi+rdx*1-8], r8
    code.extend_from_slice(&[0x48, 0xff, 0xc9]); // dec rcx
    code.extend_from_slice(&[0x75, 0xe7]); // jne loop (-25 → LOAD_ADDR)
    code.push(0xf4); // hlt

    let setup = |v: &mut X86_64Vcpu, mem: &Arc<GuestMemoryMmap>| {
        mem.write_obj(0x1111u64, GuestAddress(SRC + 0x10)).unwrap();
        mem.write_obj(0x2222u64, GuestAddress(SRC + 0x18)).unwrap();
        mem.write_obj(0u64, GuestAddress(DST + 0x10)).unwrap();
        mem.write_obj(0u64, GuestAddress(DST + 0x18)).unwrap();
        let mut r = v.get_regs().unwrap();
        r.rsi = SRC + 0x10;
        r.rdi = DST + 0x10;
        r.rdx = 0x10;
        r.rcx = 1;
        r.r8 = 0xDEAD; // sentinel: surfaces if the r8 load is dropped
        r.r9 = 0xBEEF;
        v.set_regs(&r).unwrap();
    };

    let (mut interp, imem) = make_vcpu_mem(&code);
    setup(&mut interp, &imem);
    run_interp(&mut interp);

    let (mut jit, jmem) = make_vcpu_mem(&code);
    setup(&mut jit, &jmem);
    jit.set_jit_mem(true);
    assert!(
        jit.jit_try_block().expect("jit_try_block"),
        "region should JIT"
    );
    run_interp(&mut jit);

    for (off, want) in [(0x10u64, 0x1111u64), (0x18, 0x2222)] {
        let iv: u64 = imem.read_obj(GuestAddress(DST + off)).unwrap();
        let jv: u64 = jmem.read_obj(GuestAddress(DST + off)).unwrap();
        assert_eq!(iv, want, "interp DST[{off:#x}]");
        assert_eq!(jv, want, "jit DST[{off:#x}] (stale r8 => dropped load)");
    }
}

/// Verbatim reconstruction of the kernel `__memset` region (0x82151000 in the
/// boot) with its captured entry state: rcx=1 (64-byte chunks), rdx=0x59 (89
/// bytes total → tail of 1 byte), rax=0 (fill). The JIT verifier flagged this
/// region; this test runs a FRESH interpreter vs the JIT to determine which is
/// correct (89 bytes should be zeroed by both).
#[test]
fn jit_mem_kernel_memset_region_matches_interpreter() {
    const BUF: u64 = 0x20_0000;
    const RET_HLT: u64 = LOAD_ADDR + 0x180;

    // Exact region bytes captured from the boot at 0xffffffff82151000.
    let region: &[u8] = &[
        0x48, 0xff, 0xc9, 0x48, 0x89, 0x07, 0x48, 0x89, 0x47, 0x08, 0x48, 0x89, 0x47, 0x10, 0x48,
        0x89, 0x47, 0x18, 0x48, 0x89, 0x47, 0x20, 0x48, 0x89, 0x47, 0x28, 0x48, 0x89, 0x47, 0x30,
        0x48, 0x89, 0x47, 0x38, 0x48, 0x8d, 0x7f, 0x40, 0x75, 0xd8, 0x0f, 0x1f, 0x84, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x89, 0xd1, 0x83, 0xe1, 0x38, 0x74, 0x14, 0xc1, 0xe9, 0x03, 0x66, 0x0f,
        0x1f, 0x44, 0x00, 0x00, 0xff, 0xc9, 0x48, 0x89, 0x07, 0x48, 0x8d, 0x7f, 0x08, 0x75, 0xf5,
        0x83, 0xe2, 0x07, 0x74, 0x0a, 0xff, 0xca, 0x88, 0x07, 0x48, 0x8d, 0x7f, 0x01, 0x75, 0xf6,
        0x4c, 0x89, 0xd0, 0xc3,
    ];

    let build = |fill_dst: bool| -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
        let (mut v, mem) = make_vcpu_mem(region);
        // hlt at the return target; stack returns there after the memset's ret.
        mem.write_obj(0xF4u8, GuestAddress(RET_HLT)).unwrap();
        let rsp = 0x18_0000u64;
        mem.write_obj(RET_HLT, GuestAddress(rsp)).unwrap();
        if fill_dst {
            for i in 0..0x80u64 {
                mem.write_obj(0xFFu8, GuestAddress(BUF + i)).unwrap();
            }
        }
        let mut r = v.get_regs().unwrap();
        r.rax = 0; // fill value
        r.rcx = 1; // 64-byte chunk count
        r.rdx = 0x59; // 89 bytes total
        r.rdi = BUF; // dst
        r.rsp = rsp;
        v.set_regs(&r).unwrap();
        (v, mem)
    };

    let (mut interp, imem) = build(true);
    run_interp(&mut interp);

    let (mut jit, jmem) = build(true);
    jit.set_jit_mem(true);
    // The region's hot 64-byte loop may not auto-promote in one shot; drive it.
    let _ = jit.jit_try_block();
    run_interp(&mut jit);

    // memset(BUF, 0, 89): bytes 0..89 zeroed, byte 89 (0x59) untouched (0xFF).
    for i in 0..0x80u64 {
        let ib: u8 = imem.read_obj(GuestAddress(BUF + i)).unwrap();
        let jb: u8 = jmem.read_obj(GuestAddress(BUF + i)).unwrap();
        assert_eq!(jb, ib, "BUF[{i}] jit vs interp");
        let expect = if i < 0x59 { 0u8 } else { 0xFFu8 };
        assert_eq!(jb, expect, "BUF[{i}] memset(89) result");
    }
}
