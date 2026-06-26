//! SMIR JIT × AVX-512 EVEX write-masking safety.
//!
//! The SMIR vector IR does not model the EVEX opmask (`{k}`), zeroing (`{z}`),
//! or the EVEX.b bit (memory broadcast `{1toN}` / register embedded-rounding
//! `{er}`+SAE). Two layers must keep that from ever becoming a silent miscompile
//! when a hot loop containing such an instruction is promoted to native code:
//!
//!   1. The lifter REFUSES to lift any masked/zeroing/broadcast/rounding EVEX
//!      instruction (`decode_evex_prefix` returns `LiftError::Unsupported`), so
//!      the region bails to the interpreter — which models masking correctly —
//!      regardless of the JIT op whitelist.
//!   2. (Belt-and-suspenders, exercised by `RAX_JIT_VERIFY`, not here.) The JIT
//!      verifier now also diffs ZMM/opmask state, so any future vector JIT that
//!      diverged would be caught rather than silently corrupting vector state.
//!
//! This test pins layer 1 directly (lift refusal) and end-to-end (a hot masked
//! loop is declined by the JIT and produces the correct masked result via the
//! interpreter). These paths are NOT reachable by single-instruction diff runs,
//! which never trigger hot-loop promotion.

#![cfg(all(feature = "smir-jit", target_arch = "x86_64"))]

use std::sync::Arc;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap, GuestRegionMmap, MmapRegion};

use rax::backend::emulator::x86_64::X86_64Vcpu;
use rax::cpu::{Registers, SystemRegisters, VCpu, VcpuExit};
use rax::smir::lift::SmirLifter;
use rax::smir::lift::LiftContext;
use rax::smir::lift::x86_64::X86_64Lifter;
use rax::smir::types::SourceArch;

// ---------------------------------------------------------------------------
// Layer 1: the lifter refuses masked/zeroing/broadcast/rounding EVEX.
// ---------------------------------------------------------------------------

/// Lift a single instruction; `Ok(())` if the lifter accepted it, `Err` if it
/// declined (which makes the JIT bail to the interpreter).
fn lift_one(bytes: &[u8]) -> Result<(), String> {
    let mut lifter = X86_64Lifter::default();
    let mut ctx = LiftContext::new(SourceArch::X86_64);
    lifter
        .lift_insn(0x1000, bytes, &mut ctx)
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
}

#[test]
fn lifter_accepts_plain_evex_but_refuses_mask_zero_broadcast_rounding() {
    // The common unmasked EVEX vector ops MUST still lift (the fast path stays
    // JIT-eligible once vector ops are ever whitelisted; today they bail later
    // at the GPR-only clobber gate, which is fine).
    assert!(
        lift_one(&[0x62, 0xf1, 0x7d, 0x48, 0x6f, 0xd1]).is_ok(),
        "unmasked vmovdqa32 %zmm1,%zmm2 must lift"
    );
    assert!(
        lift_one(&[0x62, 0xf1, 0x6d, 0x48, 0xfe, 0xd9]).is_ok(),
        "unmasked vpaddd %zmm1,%zmm2,%zmm3 must lift"
    );

    // Every form the SMIR vector model can't represent MUST be refused so it
    // falls back to the interpreter. (Encodings from llvm-mc.)
    let refused: &[(&str, &[u8])] = &[
        // vmovdqa32 %zmm1,%zmm2{%k1}      — write-mask (aaa=1)
        ("vmovdqa32 {k1}", &[0x62, 0xf1, 0x7d, 0x49, 0x6f, 0xd1]),
        // vmovdqa32 %zmm1,%zmm2{%k1}{z}   — zeroing (z=1, aaa=1)
        ("vmovdqa32 {k1}{z}", &[0x62, 0xf1, 0x7d, 0xc9, 0x6f, 0xd1]),
        // vpaddd %zmm1,%zmm2,%zmm3{%k1}   — masked arithmetic
        ("vpaddd {k1}", &[0x62, 0xf1, 0x6d, 0x49, 0xfe, 0xd9]),
        // vaddps (%rax){1to16},%zmm1,%zmm2 — memory broadcast (b=1, mem)
        ("vaddps {1to16}", &[0x62, 0xf1, 0x74, 0x58, 0x58, 0x10]),
        // vaddps {rn-sae},%zmm1,%zmm2,%zmm3 — embedded rounding (b=1, reg;
        // here L'L=00 would even misdecode the width as 128-bit if not bailed)
        ("vaddps {rn-sae}", &[0x62, 0xf1, 0x6c, 0x18, 0x58, 0xd9]),
    ];
    for (name, bytes) in refused {
        assert!(
            lift_one(bytes).is_err(),
            "{name} must be refused by the lifter (bytes={bytes:02x?})"
        );
    }
}

// ---------------------------------------------------------------------------
// Layer 1 end-to-end: a hot loop with a masked EVEX move is declined by the JIT
// and runs correctly on the interpreter (which honors the opmask).
// ---------------------------------------------------------------------------

const LOAD_ADDR: u64 = 0x10_0000;
const MEM_SIZE: u64 = 16 * 1024 * 1024;

fn make_vcpu(code: &[u8]) -> X86_64Vcpu {
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
    // PAE + OSFXSR + OSXSAVE so the SSE/AVX-512 state is enabled in-guest.
    sregs.cr4 = 0x20 | (1 << 9) | (1 << 18);
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

fn run_to_hlt(vcpu: &mut X86_64Vcpu) {
    for _ in 0..1_000_000 {
        match vcpu.step() {
            Ok(Some(VcpuExit::Hlt)) => return,
            Ok(_) => {}
            Err(e) => panic!("interp error: {e:?}"),
        }
    }
    panic!("guest did not halt");
}

fn set_zmm(regs: &mut Registers, idx: usize, v: [u64; 8]) {
    regs.xmm[idx] = [v[0], v[1]];
    regs.ymm_high[idx] = [v[2], v[3]];
    regs.zmm_high[idx] = [v[4], v[5], v[6], v[7]];
}

fn get_zmm(regs: &Registers, idx: usize) -> [u64; 8] {
    [
        regs.xmm[idx][0],
        regs.xmm[idx][1],
        regs.ymm_high[idx][0],
        regs.ymm_high[idx][1],
        regs.zmm_high[idx][0],
        regs.zmm_high[idx][1],
        regs.zmm_high[idx][2],
        regs.zmm_high[idx][3],
    ]
}

#[test]
fn hot_masked_evex_move_bails_to_interpreter_and_is_correct() {
    // loop:  vmovdqa32 %zmm1,%zmm2{%k1}   (62 f1 7d 49 6f d1)  dword merge-mask
    //        dec ecx                       (ff c9)
    //        jnz loop                      (75 f6  -> back 10 bytes)
    // hlt
    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf1, 0x7d, 0x49, 0x6f, 0xd1]); // vmovdqa32 %zmm1,%zmm2{%k1}
    code.extend_from_slice(&[0xff, 0xc9]); // dec ecx
    code.extend_from_slice(&[0x75, 0xf6]); // jnz loop (-10)
    code.push(0xf4); // hlt

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200; // > JIT hotness threshold (64), so promotion is attempted
    set_zmm(&mut regs, 1, [0x1111_1111_1111_1111; 8]); // src: all dwords 0x11111111
    set_zmm(&mut regs, 2, [0x2222_2222_2222_2222; 8]); // dst init: all dwords 0x22222222
    regs.k[1] = 0x5555; // dword lanes 0,2,4..14 selected; 1,3,..15 masked off
    vcpu.set_regs(&regs).unwrap();

    // Forcing a compile at the loop head must DECLINE (the masked EVEX op is not
    // liftable), so the region never runs natively.
    let jitted = vcpu.jit_try_block().expect("jit_try_block");
    assert!(
        !jitted,
        "a region containing a masked EVEX move must bail, not JIT"
    );

    // Now drive the whole hot loop on the interpreter; it must remain ineligible
    // (zero compiled regions) and produce the correct merge-masked result.
    run_to_hlt(&mut vcpu);
    let out = vcpu.get_regs().unwrap();

    assert_eq!(out.rcx & 0xffff_ffff, 0, "loop drained");
    assert_eq!(
        vcpu.jit_region_count(),
        0,
        "the masked-vector hot loop must never be JIT-promoted"
    );

    // Merge masking: dword lane j takes src (0x11111111) where k1 bit j == 1,
    // else keeps dst (0x22222222). With k1=0x5555 → low dword of each u64 is the
    // even (selected) lane = 0x11111111, high dword is the odd lane = 0x22222222.
    let expected = [0x2222_2222_1111_1111u64; 8];
    assert_eq!(
        get_zmm(&out, 2),
        expected,
        "masked move must honor k1 (got {:016x?})",
        get_zmm(&out, 2)
    );
}

#[test]
fn control_gpr_hot_loop_does_jit() {
    // Sanity: an all-GPR hot loop with the same shape DOES promote, so the
    // `!jitted` assertion above is meaningful (the harness can trigger the JIT).
    //   loop: add eax,3 ; dec ecx ; jnz loop ; hlt
    let mut code = Vec::new();
    code.extend_from_slice(&[0x83, 0xc0, 0x03]); // add eax,3
    code.extend_from_slice(&[0xff, 0xc9]); // dec ecx
    code.extend_from_slice(&[0x75, 0xf9]); // jnz loop (-7)
    code.push(0xf4); // hlt

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    regs.rax = 0;
    vcpu.set_regs(&regs).unwrap();

    let jitted = vcpu.jit_try_block().expect("jit_try_block");
    assert!(jitted, "a register-only hot loop must JIT (control)");
    run_to_hlt(&mut vcpu);
    let out = vcpu.get_regs().unwrap();
    assert_eq!(out.rax & 0xffff_ffff, 200 * 3, "control loop result");
}
