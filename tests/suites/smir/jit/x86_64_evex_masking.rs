//! SMIR JIT × AVX-512 EVEX write-masking safety.
//!
//! SMIR preserves the EVEX opmask (`{k}`) and zeroing (`{z}`) directly for
//! selected native-lowered bit-manipulation operations; other supported vector
//! families may expand masking into primitive operations. EVEX.b memory
//! broadcast (`{1toN}`) / register embedded rounding (`{er}`+SAE) remains
//! outside this JIT path. Two layers keep unsupported forms from becoming
//! silent miscompilations when a hot loop is promoted to native code:
//!
//!   1. The lifter preserves masking for explicitly modeled operations and
//!      refuses unsupported masked/zeroing/broadcast/rounding forms, so those
//!      regions bail to the interpreter regardless of the JIT op whitelist.
//!   2. (Belt-and-suspenders, exercised by `RAX_JIT_VERIFY`, not here.) The JIT
//!      verifier now also diffs ZMM/opmask state, so any future vector JIT that
//!      diverged would be caught rather than silently corrupting vector state.
//!
//! This test pins layer 1 directly (acceptance and refusal) and end-to-end (a hot
//! loop containing an unsupported masked move is declined by the JIT and
//! produces the correct result via the interpreter). These paths are not
//! reachable by single-instruction differential runs, which never trigger
//! hot-loop promotion.

#![cfg(all(feature = "smir-jit", target_arch = "x86_64"))]

use std::sync::Arc;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap, GuestRegionMmap, MmapRegion};

use rax::isa::x86_64::X86_64Vcpu;
use rax::smir::ir::types::SourceArch;
use rax::smir::lift::LiftContext;
use rax::smir::lift::SmirLifter;
use rax::smir::lift::x86_64::X86_64Lifter;
use rax::vm::vcpu::{Registers, SystemRegisters, VCpu, VcpuExit};

// ---------------------------------------------------------------------------
// Layer 1: modeled masking lifts; unsupported EVEX features are refused.
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
fn lifter_accepts_modeled_evex_masking_but_refuses_unsupported_features() {
    // Common unmasked EVEX vector ops must still lift. Operations outside the
    // explicit native vector family continue to deopt at the JIT safety gate.
    assert!(
        lift_one(&[0x62, 0xf1, 0x7d, 0x48, 0x6f, 0xd1]).is_ok(),
        "unmasked vmovdqa32 %zmm1,%zmm2 must lift"
    );
    assert!(
        lift_one(&[0x62, 0xf1, 0x6d, 0x48, 0xfe, 0xd9]).is_ok(),
        "unmasked vpaddd %zmm1,%zmm2,%zmm3 must lift"
    );

    // Native-lowered bit-manipulation operations preserve their architectural
    // opmask and zeroing fields in SMIR and must remain JIT-lowerable.
    let modeled: &[(&str, &[u8])] = &[
        // vprold $7,%zmm18,%zmm17{%k4}{z}
        (
            "vprold {k4}{z}",
            &[0x62, 0xb1, 0x75, 0xcc, 0x72, 0xca, 0x07],
        ),
        // vpternlogd $0x96,%zmm3,%zmm2,%zmm1{%k4}{z}
        (
            "vpternlogd {k4}{z}",
            &[0x62, 0xf3, 0x6d, 0xcc, 0x25, 0xcb, 0x96],
        ),
        // vpdpbusd %zmm3,%zmm2,%zmm1{%k4}{z}
        ("vpdpbusd {k4}{z}", &[0x62, 0xf2, 0x6d, 0xcc, 0x50, 0xcb]),
        // vpmadd52luq %zmm3,%zmm2,%zmm1{%k4}{z}
        ("vpmadd52luq {k4}{z}", &[0x62, 0xf2, 0xed, 0xcc, 0xb4, 0xcb]),
        // vdpbf16ps %zmm3,%zmm2,%zmm1{%k4}{z}
        ("vdpbf16ps {k4}{z}", &[0x62, 0xf2, 0x6e, 0xcc, 0x52, 0xcb]),
        // vcvtneps2bf16 %zmm2,%ymm1{%k4}{z}
        (
            "vcvtneps2bf16 {k4}{z}",
            &[0x62, 0xf2, 0x7e, 0xcc, 0x72, 0xca],
        ),
        // vaddph %zmm3,%zmm2,%zmm1{%k4}{z}
        ("vaddph {k4}{z}", &[0x62, 0xf5, 0x6c, 0xcc, 0x58, 0xcb]),
        // vpconflictd %zmm2,%zmm1{%k4}{z}
        ("vpconflictd {k4}{z}", &[0x62, 0xf2, 0x7d, 0xcc, 0xc4, 0xca]),
        // vplzcntd %zmm2,%zmm1{%k4}{z}
        ("vplzcntd {k4}{z}", &[0x62, 0xf2, 0x7d, 0xcc, 0x44, 0xca]),
        // vpermb %zmm3,%zmm2,%zmm1{%k4}{z}
        ("vpermb {k4}{z}", &[0x62, 0xf2, 0x6d, 0xcc, 0x8d, 0xcb]),
        ("vpcompressd {k4}{z}", &[0x62, 0xf2, 0x7d, 0xcc, 0x8b, 0xd1]),
        ("vpexpandd {k4}{z}", &[0x62, 0xf2, 0x7d, 0xcc, 0x89, 0xca]),
        ("vpcompressb {k4}{z}", &[0x62, 0xf2, 0x7d, 0xcc, 0x63, 0xd1]),
        ("vpexpandw {k3}{z}", &[0x62, 0xd2, 0xfd, 0x8b, 0x62, 0xf9]),
        ("vpmovusdb {k4}{z}", &[0x62, 0xf2, 0x7e, 0xcc, 0x11, 0xd1]),
        ("vaesenc zmm16", &[0x62, 0xa2, 0x75, 0x40, 0xdc, 0xc2]),
        ("vaesenclast zmm1", &[0x62, 0xf2, 0x6d, 0x48, 0xdd, 0xcb]),
        ("vaesdec ymm4", &[0xc4, 0xe2, 0x55, 0xde, 0xe6]),
        ("vaesdeclast xmm7", &[0xc4, 0xc2, 0x39, 0xdf, 0xf9]),
        ("vaesimc xmm9", &[0xc4, 0x42, 0x79, 0xdb, 0xc8]),
        (
            "vaeskeygenassist xmm11",
            &[0xc4, 0x43, 0x79, 0xdf, 0xda, 0x5a],
        ),
        ("vsha512msg1 ymm9", &[0xc4, 0x42, 0x7f, 0xcc, 0xca]),
        ("vsha512msg2 ymm9", &[0xc4, 0x42, 0x7f, 0xcd, 0xca]),
        ("vsha512rnds2 ymm9", &[0xc4, 0x42, 0x27, 0xcb, 0xca]),
        ("vsm3msg1 xmm9", &[0xc4, 0x42, 0x20, 0xda, 0xca]),
        ("vsm3msg2 xmm9", &[0xc4, 0x42, 0x21, 0xda, 0xca]),
        ("vsm3rnds2 xmm9", &[0xc4, 0x43, 0x21, 0xde, 0xca, 0x3e]),
        ("vsm4key4 xmm1", &[0xc4, 0xe2, 0x6a, 0xda, 0xcb]),
        ("vsm4key4 ymm4", &[0xc4, 0xe2, 0x56, 0xda, 0xe6]),
        ("vsm4rnds4 xmm7", &[0xc4, 0xc2, 0x3b, 0xda, 0xf9]),
        ("vsm4rnds4 ymm10", &[0xc4, 0x42, 0x27, 0xda, 0xd4]),
        ("vpsrlw xmm1", &[0xc5, 0xf1, 0x71, 0xd2, 0x03]),
        ("vpsraw ymm4", &[0xc5, 0xdd, 0x71, 0xe5, 0x04]),
        ("vpslld zmm17", &[0x62, 0xb1, 0x75, 0x40, 0x72, 0xf2, 0x05]),
        ("vpsraq xmm17", &[0x62, 0xb1, 0xf5, 0x00, 0x72, 0xe2, 0x09]),
        ("vpslldq xmm9", &[0xc4, 0xc1, 0x31, 0x73, 0xfa, 0x07]),
        ("vpsrldq ymm11", &[0xc4, 0xc1, 0x25, 0x73, 0xdc, 0x08]),
        ("vpsrlw xmm1 shared count", &[0xc5, 0xe9, 0xd1, 0xcb]),
        ("vpsraw ymm4 shared count", &[0xc5, 0xd5, 0xe1, 0xe6]),
        (
            "vpsllq zmm17 shared count",
            &[0x62, 0xa1, 0xed, 0x40, 0xf3, 0xcb],
        ),
        (
            "vpsraq zmm17 shared count",
            &[0x62, 0xa1, 0xed, 0x40, 0xe2, 0xcb],
        ),
    ];
    for (name, bytes) in modeled {
        assert!(
            lift_one(bytes).is_ok(),
            "modeled {name} must lift (bytes={bytes:02x?})"
        );
    }

    // General masked arithmetic is also semantically liftable, but currently
    // expands through virtual-vector mask/select operations and therefore stays
    // outside the native identity-map clobber gate.
    assert!(
        lift_one(&[0x62, 0xf1, 0x6d, 0x49, 0xfe, 0xd9]).is_ok(),
        "masked vpaddd must remain interpreter-liftable"
    );
    assert!(
        lift_one(&[0x62, 0xf1, 0x74, 0x58, 0x58, 0x10]).is_ok(),
        "broadcast vaddps must remain interpreter-liftable"
    );

    // Every form this SMIR vector path cannot represent must be refused so it
    // falls back to the interpreter. (Encodings from llvm-mc.)
    let refused: &[(&str, &[u8])] = &[
        // vmovdqa32 %zmm1,%zmm2{%k1}      — write-mask (aaa=1)
        ("vmovdqa32 {k1}", &[0x62, 0xf1, 0x7d, 0x49, 0x6f, 0xd1]),
        // vmovdqa32 %zmm1,%zmm2{%k1}{z}   — zeroing (z=1, aaa=1)
        ("vmovdqa32 {k1}{z}", &[0x62, 0xf1, 0x7d, 0xc9, 0x6f, 0xd1]),
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
    if idx < 16 {
        regs.xmm[idx] = [v[0], v[1]];
        regs.ymm_high[idx] = [v[2], v[3]];
        regs.zmm_high[idx] = [v[4], v[5], v[6], v[7]];
    } else {
        regs.zmm_ext[idx - 16] = v;
    }
}

fn get_zmm(regs: &Registers, idx: usize) -> [u64; 8] {
    if idx < 16 {
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
    } else {
        regs.zmm_ext[idx - 16]
    }
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
fn hot_masked_rotate_jits_and_round_trips_zmm_and_opmask_state() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    // loop: vprold $7,%zmm2,%zmm1{%k4}{z}
    //       dec ecx
    //       jnz loop
    // hlt
    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf1, 0x75, 0xcc, 0x72, 0xca, 0x07]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf5]); // back 11 bytes
    code.push(0xf4);

    let source = [
        0x0123_4567_89ab_cdef,
        0x1111_2222_3333_4444,
        0x8000_0001_7fff_ffff,
        0xdead_beef_cafe_babe,
        0x0102_0304_0506_0708,
        0xf0e0_d0c0_b0a0_9080,
        0x1357_9bdf_2468_ace0,
        0xffff_ffff_0000_0001,
    ];
    let mask = 0x5555u64;
    let mut expected = [0u64; 8];
    for lane in 0..16 {
        let input = (source[lane / 2] >> ((lane % 2) * 32)) as u32;
        let output = if ((mask >> lane) & 1) != 0 {
            input.rotate_left(7)
        } else {
            0
        };
        expected[lane / 2] |= (output as u64) << ((lane % 2) * 32);
    }

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 1, [u64::MAX; 8]);
    set_zmm(&mut regs, 2, source);
    regs.k[4] = mask;
    vcpu.set_regs(&regs).unwrap();

    assert!(
        vcpu.jit_try_block().expect("jit masked VPROLD loop"),
        "a modeled register-only masked rotate loop must enter the native tier"
    );
    let after_jit = vcpu.get_regs().unwrap();
    assert_eq!(after_jit.rcx & 0xffff_ffff, 0, "native loop drained");
    assert_eq!(get_zmm(&after_jit, 1), expected);
    assert_eq!(get_zmm(&after_jit, 2), source, "source ZMM survived");
    assert_eq!(after_jit.k[4], mask, "source opmask survived");

    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_masked_vpopcntd_jits_with_direct_mask_semantics() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vpopcntdq")
    {
        return;
    }

    // loop: vpopcntd %zmm2,%zmm1{%k4}{z}
    //       dec ecx
    //       jnz loop
    // hlt
    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf2, 0x7d, 0xcc, 0x55, 0xca]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf6]); // back 10 bytes
    code.push(0xf4);

    let source = [
        0x0123_4567_89ab_cdef,
        0x1111_2222_3333_4444,
        0x8000_0001_7fff_ffff,
        0xdead_beef_cafe_babe,
        0x0102_0304_0506_0708,
        0xf0e0_d0c0_b0a0_9080,
        0x1357_9bdf_2468_ace0,
        0xffff_ffff_0000_0001,
    ];
    let mask = 0x9669u64;
    let mut expected = [0u64; 8];
    for lane in 0..16 {
        let input = (source[lane / 2] >> ((lane % 2) * 32)) as u32;
        let output = if ((mask >> lane) & 1) != 0 {
            input.count_ones()
        } else {
            0
        };
        expected[lane / 2] |= (output as u64) << ((lane % 2) * 32);
    }

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 1, [u64::MAX; 8]);
    set_zmm(&mut regs, 2, source);
    regs.k[4] = mask;
    vcpu.set_regs(&regs).unwrap();

    assert!(
        vcpu.jit_try_block().expect("jit masked VPOPCNTD loop"),
        "a modeled register-only masked VPOPCNTD loop must enter the native tier"
    );
    let after_jit = vcpu.get_regs().unwrap();
    assert_eq!(after_jit.rcx & 0xffff_ffff, 0, "native loop drained");
    assert_eq!(get_zmm(&after_jit, 1), expected);
    assert_eq!(get_zmm(&after_jit, 2), source, "source ZMM survived");
    assert_eq!(after_jit.k[4], mask, "source opmask survived");

    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_masked_vpermb_jits_with_direct_table_and_index_roles() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vbmi")
    {
        return;
    }

    // loop: vpermb %zmm3,%zmm2,%zmm1{%k4}{z}
    //       dec ecx
    //       jnz loop
    // hlt
    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf2, 0x6d, 0xcc, 0x8d, 0xcb]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf6]);
    code.push(0xf4);

    let table_bytes = std::array::from_fn::<_, 64, _>(|lane| (lane as u8).wrapping_mul(3) ^ 0xA5);
    let index_bytes = std::array::from_fn::<_, 64, _>(|lane| (63 - lane) as u8);
    let pack = |bytes: &[u8; 64]| {
        std::array::from_fn::<_, 8, _>(|word| {
            u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
        })
    };
    let table = pack(&table_bytes);
    let indices = pack(&index_bytes);
    let mask = 0xA55A_9669_F00F_5AA5u64;
    let expected_bytes = std::array::from_fn::<_, 64, _>(|lane| {
        if ((mask >> lane) & 1) != 0 {
            table_bytes[(index_bytes[lane] & 63) as usize]
        } else {
            0
        }
    });
    let expected = pack(&expected_bytes);

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 1, [u64::MAX; 8]);
    set_zmm(&mut regs, 2, indices);
    set_zmm(&mut regs, 3, table);
    regs.k[4] = mask;
    vcpu.set_regs(&regs).unwrap();

    assert!(vcpu.jit_try_block().expect("jit masked VPERMB loop"));
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xffff_ffff, 0);
    assert_eq!(get_zmm(&after, 1), expected);
    assert_eq!(get_zmm(&after, 2), indices);
    assert_eq!(get_zmm(&after, 3), table);
    assert_eq!(after.k[4], mask);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_masked_vpcompressd_jits_with_packed_lane_semantics() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }
    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf2, 0x7d, 0xcc, 0x8b, 0xd1]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf6]);
    code.push(0xf4);
    let lanes = std::array::from_fn::<_, 16, _>(|lane| 0x1000_0000u32 + lane as u32);
    let mut source = [0u64; 8];
    for (lane, value) in lanes.iter().copied().enumerate() {
        source[lane / 2] |= u64::from(value) << ((lane % 2) * 32);
    }
    let mask = 0xA55Au64;
    let selected = lanes
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(lane, value)| ((mask >> lane) & 1 != 0).then_some(value))
        .collect::<Vec<_>>();
    let mut expected = [0u64; 8];
    for (lane, value) in selected.into_iter().enumerate() {
        expected[lane / 2] |= u64::from(value) << ((lane % 2) * 32);
    }
    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 1, [u64::MAX; 8]);
    set_zmm(&mut regs, 2, source);
    regs.k[4] = mask;
    vcpu.set_regs(&regs).unwrap();
    assert!(vcpu.jit_try_block().expect("jit masked VPCOMPRESSD loop"));
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xffff_ffff, 0);
    assert_eq!(get_zmm(&after, 1), expected);
    assert_eq!(get_zmm(&after, 2), source);
    assert_eq!(after.k[4], mask);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_masked_vpcompressb_jits_with_vbmi2_packed_lane_semantics() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vbmi2")
    {
        return;
    }
    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf2, 0x7d, 0xcc, 0x63, 0xd1]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf6]);
    code.push(0xf4);
    let bytes = std::array::from_fn::<_, 64, _>(|lane| (lane as u8).wrapping_mul(5) ^ 0xA3);
    let pack = |input: &[u8; 64]| {
        std::array::from_fn::<_, 8, _>(|word| {
            u64::from_le_bytes(input[word * 8..word * 8 + 8].try_into().unwrap())
        })
    };
    let source = pack(&bytes);
    let mask = 0xA55A_9669_F00F_5AA5u64;
    let mut expected_bytes = [0u8; 64];
    let mut output = 0;
    for (lane, value) in bytes.iter().copied().enumerate() {
        if ((mask >> lane) & 1) != 0 {
            expected_bytes[output] = value;
            output += 1;
        }
    }
    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 1, [u64::MAX; 8]);
    set_zmm(&mut regs, 2, source);
    regs.k[4] = mask;
    vcpu.set_regs(&regs).unwrap();
    assert!(vcpu.jit_try_block().expect("jit masked VPCOMPRESSB loop"));
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xffff_ffff, 0);
    assert_eq!(get_zmm(&after, 1), pack(&expected_bytes));
    assert_eq!(get_zmm(&after, 2), source);
    assert_eq!(after.k[4], mask);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_masked_vpmovusdb_jits_with_saturating_narrow_semantics() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }
    // Rosetta 2 currently applies signed-to-unsigned saturation to this opcode,
    // while the ISA defines an unsigned source. Keep the silicon regression
    // active and avoid treating that translation discrepancy as x86 behavior.
    #[cfg(target_os = "macos")]
    if std::process::Command::new("/usr/sbin/sysctl")
        .args(["-in", "sysctl.proc_translated"])
        .output()
        .is_ok_and(|output| output.stdout == b"1\n")
    {
        return;
    }
    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf2, 0x7e, 0xcc, 0x11, 0xd1]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf6]);
    code.push(0xf4);
    let lanes = [
        i32::MIN,
        -1,
        0,
        1,
        127,
        128,
        254,
        255,
        256,
        511,
        42,
        -42,
        0x7fff,
        7,
        300,
        i32::MAX,
    ];
    let mut source = [0u64; 8];
    for (lane, value) in lanes.iter().copied().enumerate() {
        source[lane / 2] |= u64::from(value as u32) << ((lane % 2) * 32);
    }
    let mask = 0xA55Au64;
    let mut expected_bytes = [0u8; 64];
    for (lane, value) in lanes.iter().copied().enumerate() {
        expected_bytes[lane] = if ((mask >> lane) & 1) == 0 {
            0
        } else {
            (value as u32).min(255) as u8
        };
    }
    let expected = std::array::from_fn::<_, 8, _>(|word| {
        u64::from_le_bytes(expected_bytes[word * 8..word * 8 + 8].try_into().unwrap())
    });
    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 1, [u64::MAX; 8]);
    set_zmm(&mut regs, 2, source);
    regs.k[4] = mask;
    vcpu.set_regs(&regs).unwrap();
    assert!(vcpu.jit_try_block().expect("jit masked VPMOVUSDB loop"));
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xffff_ffff, 0);
    assert_eq!(get_zmm(&after, 1), expected);
    assert_eq!(get_zmm(&after, 2), source);
    assert_eq!(after.k[4], mask);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_vaesenc_jits_with_independent_128_bit_lane_semantics() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("vaes")
    {
        return;
    }
    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xF2, 0x6D, 0x48, 0xDC, 0xCB]);
    code.extend_from_slice(&[0xFF, 0xC9]);
    code.extend_from_slice(&[0x75, 0xF6]);
    code.push(0xF4);

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 1, [u64::MAX; 8]);
    set_zmm(&mut regs, 2, [0; 8]);
    set_zmm(&mut regs, 3, [0; 8]);
    vcpu.set_regs(&regs).unwrap();

    assert!(vcpu.jit_try_block().expect("jit VAESENC loop"));
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xFFFF_FFFF, 0);
    assert_eq!(get_zmm(&after, 1), [0x6363_6363_6363_6363; 8]);
    assert_eq!(get_zmm(&after, 2), [0; 8]);
    assert_eq!(get_zmm(&after, 3), [0; 8]);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_vaeskeygenassist_jits_with_vex_upper_zeroing_semantics() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("aes")
    {
        return;
    }
    let mut code = Vec::new();
    code.extend_from_slice(&[0xC4, 0xE3, 0x79, 0xDF, 0xCA, 0x00]);
    code.extend_from_slice(&[0xFF, 0xC9]);
    code.extend_from_slice(&[0x75, 0xF6]);
    code.push(0xF4);

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 1, [u64::MAX; 8]);
    set_zmm(&mut regs, 2, [0; 8]);
    vcpu.set_regs(&regs).unwrap();

    assert!(vcpu.jit_try_block().expect("jit VAESKEYGENASSIST loop"));
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xFFFF_FFFF, 0);
    assert_eq!(
        get_zmm(&after, 1),
        [
            0x6363_6363_6363_6363,
            0x6363_6363_6363_6363,
            0,
            0,
            0,
            0,
            0,
            0,
        ]
    );
    assert_eq!(get_zmm(&after, 2), [0; 8]);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_vsha512msg1_jits_with_schedule_and_vex_upper_zeroing_semantics() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx2")
        || !std::is_x86_feature_detected!("sha512")
    {
        return;
    }
    let mut code = Vec::new();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7F, 0xCC, 0xCA]);
    code.extend_from_slice(&[0xFF, 0xC9]);
    code.extend_from_slice(&[0x75, 0xF7]);
    code.push(0xF4);

    let source = [
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        0xA5A5_A5A5_A5A5_A5A5,
        0x5A5A_5A5A_5A5A_5A5A,
        1,
        2,
        3,
        4,
    ];
    let initial = [
        0x6A09_E667_F3BC_C908,
        0xBB67_AE85_84CA_A73B,
        0x3C6E_F372_FE94_F82B,
        0xA54F_F53A_5F1D_36F1,
        u64::MAX,
        u64::MAX,
        u64::MAX,
        u64::MAX,
    ];
    let sigma0 = |value: u64| value.rotate_right(1) ^ value.rotate_right(8) ^ (value >> 7);
    let mut expected = initial;
    for _ in 0..200 {
        expected = [
            expected[0].wrapping_add(sigma0(expected[1])),
            expected[1].wrapping_add(sigma0(expected[2])),
            expected[2].wrapping_add(sigma0(expected[3])),
            expected[3].wrapping_add(sigma0(source[0])),
            0,
            0,
            0,
            0,
        ];
    }

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 1, initial);
    set_zmm(&mut regs, 2, source);
    vcpu.set_regs(&regs).unwrap();

    assert!(vcpu.jit_try_block().expect("jit VSHA512MSG1 loop"));
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xFFFF_FFFF, 0);
    assert_eq!(get_zmm(&after, 1), expected);
    assert_eq!(get_zmm(&after, 2), source);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_vsm3msg1_jits_with_schedule_and_vex_upper_zeroing_semantics() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx")
        || !std::is_x86_feature_detected!("sm3")
    {
        return;
    }
    let mut code = Vec::new();
    code.extend_from_slice(&[0xC4, 0xE2, 0x68, 0xDA, 0xCB]);
    code.extend_from_slice(&[0xFF, 0xC9]);
    code.extend_from_slice(&[0x75, 0xF7]);
    code.push(0xF4);

    let initial_words = [0x0123_4567u32, 0x89AB_CDEF, 0x0F1E_2D3C, 0x4B5A_6978];
    let first_words = [0x1122_3344u32, 0x5566_7788, 0x99AA_BBCC, 0xDDEE_FF00];
    let second_words = [0x1357_9BDFu32, 0x2468_ACE0, 0xF0E1_D2C3, 0xB4A5_9687];
    let pack = |words: [u32; 4], upper: u64| {
        [
            u64::from(words[0]) | (u64::from(words[1]) << 32),
            u64::from(words[2]) | (u64::from(words[3]) << 32),
            upper,
            upper,
            upper,
            upper,
            upper,
            upper,
        ]
    };
    let mut expected_words = initial_words;
    let p1 = |value: u32| value ^ value.rotate_left(15) ^ value.rotate_left(23);
    for _ in 0..200 {
        let old = expected_words;
        expected_words = std::array::from_fn(|index| {
            let mut value = old[index] ^ second_words[index];
            if index < 3 {
                value ^= first_words[index].rotate_left(15);
            }
            p1(value)
        });
    }
    let initial = pack(initial_words, u64::MAX);
    let first = pack(first_words, 0xA5A5_A5A5_A5A5_A5A5);
    let second = pack(second_words, 0x5A5A_5A5A_5A5A_5A5A);
    let expected = pack(expected_words, 0);

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 1, initial);
    set_zmm(&mut regs, 2, first);
    set_zmm(&mut regs, 3, second);
    vcpu.set_regs(&regs).unwrap();

    assert!(vcpu.jit_try_block().expect("jit VSM3MSG1 loop"));
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xFFFF_FFFF, 0);
    assert_eq!(get_zmm(&after, 1), expected);
    assert_eq!(get_zmm(&after, 2), first);
    assert_eq!(get_zmm(&after, 3), second);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_vsm4rnds4_jits_with_standard_vector_and_vex_upper_zeroing() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx2")
        || !std::is_x86_feature_detected!("sm4")
    {
        return;
    }
    let mut code = Vec::new();
    code.extend_from_slice(&[0xC4, 0xE2, 0x6B, 0xDA, 0xCB]);
    code.extend_from_slice(&[0xFF, 0xC9]);
    code.extend_from_slice(&[0x75, 0xF7]);
    code.push(0xF4);
    let pack = |words: [u32; 4], upper: u64| {
        [
            u64::from(words[0]) | (u64::from(words[1]) << 32),
            u64::from(words[2]) | (u64::from(words[3]) << 32),
            upper,
            upper,
            upper,
            upper,
            upper,
            upper,
        ]
    };
    let plaintext = pack(
        [0x0123_4567, 0x89AB_CDEF, 0xFEDC_BA98, 0x7654_3210],
        0xA5A5_A5A5_A5A5_A5A5,
    );
    let round_keys = pack(
        [0xF121_86F9, 0x4166_2B61, 0x5A6A_B19A, 0x7BA9_2077],
        0x5A5A_5A5A_5A5A_5A5A,
    );
    let expected = pack([0x27FA_D345, 0xA18B_4CB2, 0x11C1_E22A, 0xCC13_E2EE], 0);
    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 1, [u64::MAX; 8]);
    set_zmm(&mut regs, 2, plaintext);
    set_zmm(&mut regs, 3, round_keys);
    vcpu.set_regs(&regs).unwrap();
    assert!(vcpu.jit_try_block().expect("jit VSM4RNDS4 loop"));
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xFFFF_FFFF, 0);
    assert_eq!(get_zmm(&after, 1), expected);
    assert_eq!(get_zmm(&after, 2), plaintext);
    assert_eq!(get_zmm(&after, 3), round_keys);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_vpsraw_imm_jits_with_vex_lane_and_upper_zeroing_semantics() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx2")
    {
        return;
    }
    let mut code = Vec::new();
    code.extend_from_slice(&[0xC5, 0xDD, 0x71, 0xE5, 0x04]);
    code.extend_from_slice(&[0xFF, 0xC9]);
    code.extend_from_slice(&[0x75, 0xF7]);
    code.push(0xF4);
    let lanes = [
        i16::MIN,
        -32767,
        -1025,
        -17,
        -16,
        -1,
        0,
        1,
        15,
        16,
        255,
        256,
        1024,
        4096,
        16384,
        i16::MAX,
    ];
    let pack = |values: [i16; 16], upper: u64| {
        let mut result = [0; 8];
        result[4..].fill(upper);
        for (lane, value) in values.into_iter().enumerate() {
            result[lane / 4] |= u64::from(value as u16) << ((lane % 4) * 16);
        }
        result
    };
    let source = pack(lanes, 0xA5A5_A5A5_A5A5_A5A5);
    let expected = pack(lanes.map(|value| value >> 4), 0);
    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 4, [u64::MAX; 8]);
    set_zmm(&mut regs, 5, source);
    vcpu.set_regs(&regs).unwrap();
    assert!(vcpu.jit_try_block().expect("jit VPSRAW immediate loop"));
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xFFFF_FFFF, 0);
    assert_eq!(get_zmm(&after, 4), expected);
    assert_eq!(get_zmm(&after, 5), source);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_vpsraq_imm_jits_with_evex_signed_lane_semantics() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }
    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xB1, 0xF5, 0x40, 0x72, 0xE2, 0x09]);
    code.extend_from_slice(&[0xFF, 0xC9]);
    code.extend_from_slice(&[0x75, 0xF5]);
    code.push(0xF4);
    let source = [
        i64::MIN as u64,
        (-1i64) as u64,
        (-513i64) as u64,
        0,
        1,
        511,
        512,
        i64::MAX as u64,
    ];
    let expected = source.map(|value| ((value as i64) >> 9) as u64);
    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 17, [u64::MAX; 8]);
    set_zmm(&mut regs, 18, source);
    vcpu.set_regs(&regs).unwrap();
    assert!(vcpu.jit_try_block().expect("jit VPSRAQ immediate loop"));
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xFFFF_FFFF, 0);
    assert_eq!(get_zmm(&after, 17), expected);
    assert_eq!(get_zmm(&after, 18), source);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_vpsraw_shared_count_jits_with_low64_count_and_vex_upper_zeroing() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx2")
    {
        return;
    }
    let mut code = Vec::new();
    // vpsraw %xmm6,%ymm5,%ymm4
    code.extend_from_slice(&[0xC5, 0xD5, 0xE1, 0xE6]);
    code.extend_from_slice(&[0xFF, 0xC9]);
    code.extend_from_slice(&[0x75, 0xF8]);
    code.push(0xF4);
    let lanes = [
        i16::MIN,
        -32767,
        -1025,
        -17,
        -16,
        -1,
        0,
        1,
        15,
        16,
        255,
        256,
        1024,
        4096,
        16384,
        i16::MAX,
    ];
    let pack = |values: [i16; 16], upper: u64| {
        let mut result = [0; 8];
        result[4..].fill(upper);
        for (lane, value) in values.into_iter().enumerate() {
            result[lane / 4] |= u64::from(value as u16) << ((lane % 4) * 16);
        }
        result
    };
    let source = pack(lanes, 0xA5A5_A5A5_A5A5_A5A5);
    let expected = pack(lanes.map(|value| value >> 4), 0);
    let count = [
        4,
        u64::MAX,
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        1,
        2,
        3,
        4,
    ];
    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 4, [u64::MAX; 8]);
    set_zmm(&mut regs, 5, source);
    set_zmm(&mut regs, 6, count);
    vcpu.set_regs(&regs).unwrap();
    assert!(vcpu.jit_try_block().expect("jit VPSRAW shared-count loop"));
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xFFFF_FFFF, 0);
    assert_eq!(get_zmm(&after, 4), expected);
    assert_eq!(get_zmm(&after, 5), source);
    assert_eq!(get_zmm(&after, 6), count);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_vpsraq_shared_count_jits_with_evex_signed_lane_semantics() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }
    let mut code = Vec::new();
    // vpsraq %xmm19,%zmm18,%zmm17
    code.extend_from_slice(&[0x62, 0xA1, 0xED, 0x40, 0xE2, 0xCB]);
    code.extend_from_slice(&[0xFF, 0xC9]);
    code.extend_from_slice(&[0x75, 0xF6]);
    code.push(0xF4);
    let source = [
        i64::MIN as u64,
        (-1i64) as u64,
        (-513i64) as u64,
        0,
        1,
        511,
        512,
        i64::MAX as u64,
    ];
    let expected = source.map(|value| ((value as i64) >> 9) as u64);
    let count = [
        9,
        u64::MAX,
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        1,
        2,
        3,
        4,
    ];
    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 17, [u64::MAX; 8]);
    set_zmm(&mut regs, 18, source);
    set_zmm(&mut regs, 19, count);
    vcpu.set_regs(&regs).unwrap();
    assert!(vcpu.jit_try_block().expect("jit VPSRAQ shared-count loop"));
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xFFFF_FFFF, 0);
    assert_eq!(get_zmm(&after, 17), expected);
    assert_eq!(get_zmm(&after, 18), source);
    assert_eq!(get_zmm(&after, 19), count);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_vpshufbitqmb_jits_and_writes_architectural_k_destination() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512bitalg")
    {
        return;
    }

    // loop: vpshufbitqmb %zmm2,%zmm3,%k5{%k1}
    //       dec ecx
    //       jnz loop
    // hlt
    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf2, 0x65, 0x49, 0x8f, 0xea]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf6]);
    code.push(0xf4);

    let source = [
        0x8000_0000_0000_0001,
        0x0123_4567_89ab_cdef,
        0xfedc_ba98_7654_3210,
        0xaaaa_5555_f0f0_0f0f,
        0x0102_0408_1020_4080,
        0x7fff_ffff_ffff_fffe,
        0x1357_9bdf_2468_ace0,
        0xffff_0000_ffff_0000,
    ];
    let indices = [
        0x3f_00_3e_01_3d_02_3c_03,
        0x04_05_06_07_08_09_0a_0b,
        0x0c_0d_0e_0f_10_11_12_13,
        0x14_15_16_17_18_19_1a_1b,
        0x1c_1d_1e_1f_20_21_22_23,
        0x24_25_26_27_28_29_2a_2b,
        0x2c_2d_2e_2f_30_31_32_33,
        0x34_35_36_37_38_39_3a_3b,
    ];
    let write_mask = 0xa55a_9669_5aa5_6996u64;
    let mut raw = 0u64;
    for qword in 0..8 {
        for byte in 0..8 {
            let index = ((indices[qword] >> (byte * 8)) & 0x3f) as u32;
            let bit = (source[qword] >> index) & 1;
            raw |= bit << (qword * 8 + byte);
        }
    }
    let expected = raw & write_mask;

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 2, indices);
    set_zmm(&mut regs, 3, source);
    regs.k[1] = write_mask;
    regs.k[5] = u64::MAX;
    vcpu.set_regs(&regs).unwrap();

    assert!(
        vcpu.jit_try_block().expect("jit VPSHUFBITQMB loop"),
        "a register-only VPSHUFBITQMB loop must enter the native tier"
    );
    let after_jit = vcpu.get_regs().unwrap();
    assert_eq!(after_jit.rcx & 0xffff_ffff, 0, "native loop drained");
    assert_eq!(after_jit.k[5], expected, "K destination write-back");
    assert_eq!(after_jit.k[1], write_mask, "write mask survived");
    assert_eq!(get_zmm(&after_jit, 2), indices, "index ZMM survived");
    assert_eq!(get_zmm(&after_jit, 3), source, "source ZMM survived");

    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_masked_vpdpbusd_jits_with_destructive_accumulator_semantics() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512vnni")
    {
        return;
    }

    // loop: vpdpbusd %zmm3,%zmm2,%zmm1{%k4}{z}
    //       dec ecx
    //       jnz loop
    // hlt
    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf2, 0x6d, 0xcc, 0x50, 0xcb]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf6]);
    code.push(0xf4);

    let accumulator = [
        0x0000_0002_0000_0001,
        0xffff_fffc_0000_0003,
        0x7fff_fff0_ffff_fffb,
        0x0000_0007_8000_0010,
        0x1111_1111_2222_2222,
        0x3333_3333_4444_4444,
        0x5555_5555_6666_6666,
        0x7777_7777_8888_8888,
    ];
    let unsigned_source = [
        0x0807_0605_0403_0201,
        0x100f_0e0d_0c0b_0a09,
        0x1817_1615_1413_1211,
        0x201f_1e1d_1c1b_1a19,
        0x2827_2625_2423_2221,
        0x302f_2e2d_2c2b_2a29,
        0x3837_3635_3433_3231,
        0x403f_3e3d_3c3b_3a39,
    ];
    let signed_source = [
        0xfc03_fe01_04fd_02ff,
        0x08f7_06f9_04fb_02fd,
        0x7f80_01ff_05fb_03fd,
        0xf00f_f20d_f40b_f609,
        0x817f_827e_837d_847c,
        0x8878_8977_8a76_8b75,
        0x906f_916e_926d_936c,
        0x9867_9966_9a65_9b64,
    ];
    let mask = 0xa55au64;
    let iterations = 200u32;
    let mut expected = [0u64; 8];
    for lane in 0..16 {
        if ((mask >> lane) & 1) == 0 {
            continue;
        }
        let mut dot = 0i32;
        for term in 0..4 {
            let byte_index = lane * 4 + term;
            let unsigned =
                ((unsigned_source[byte_index / 8] >> ((byte_index % 8) * 8)) & 0xff) as u8;
            let signed =
                (((signed_source[byte_index / 8] >> ((byte_index % 8) * 8)) & 0xff) as u8) as i8;
            dot = dot.wrapping_add(i32::from(unsigned) * i32::from(signed));
        }
        let initial = ((accumulator[lane / 2] >> ((lane % 2) * 32)) & 0xffff_ffff) as u32;
        let result = initial.wrapping_add((dot as u32).wrapping_mul(iterations));
        expected[lane / 2] |= (result as u64) << ((lane % 2) * 32);
    }

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = u64::from(iterations);
    set_zmm(&mut regs, 1, accumulator);
    set_zmm(&mut regs, 2, unsigned_source);
    set_zmm(&mut regs, 3, signed_source);
    regs.k[4] = mask;
    vcpu.set_regs(&regs).unwrap();

    assert!(
        vcpu.jit_try_block().expect("jit masked VPDPBUSD loop"),
        "a register-only masked VPDPBUSD loop must enter the native tier"
    );
    let after_jit = vcpu.get_regs().unwrap();
    assert_eq!(after_jit.rcx & 0xffff_ffff, 0, "native loop drained");
    assert_eq!(get_zmm(&after_jit, 1), expected, "accumulator write-back");
    assert_eq!(
        get_zmm(&after_jit, 2),
        unsigned_source,
        "unsigned source survived"
    );
    assert_eq!(
        get_zmm(&after_jit, 3),
        signed_source,
        "signed source survived"
    );
    assert_eq!(after_jit.k[4], mask, "source opmask survived");

    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_masked_vpmadd52luq_jits_with_52_bit_product_semantics() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512ifma")
    {
        return;
    }

    // loop: vpmadd52luq %zmm3,%zmm2,%zmm1{%k4}{z}
    //       dec ecx
    //       jnz loop
    // hlt
    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf2, 0xed, 0xcc, 0xb4, 0xcb]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf6]);
    code.push(0xf4);

    const MASK52: u64 = (1u64 << 52) - 1;
    let accumulator = [
        1,
        u64::MAX - 7,
        0x1111_2222_3333_4444,
        0x5555_6666_7777_8888,
        0x9999_aaaa_bbbb_cccc,
        0xdddd_eeee_ffff_0000,
        0x0123_4567_89ab_cdef,
        0xfedc_ba98_7654_3210,
    ];
    let lhs = [
        MASK52,
        0x000f_edcb_a987_6543,
        0x0001_2345_6789_abcd,
        u64::MAX,
        3,
        5,
        7,
        11,
    ];
    let rhs = [
        3,
        0x000f_ffff_ffff_fff1,
        0x000a_bcde_f012_3456,
        MASK52,
        13,
        17,
        19,
        23,
    ];
    let mask = 0xa5u64;
    let iterations = 200u64;
    let mut expected = [0u64; 8];
    for lane in 0..8 {
        if ((mask >> lane) & 1) != 0 {
            let product = u128::from(lhs[lane] & MASK52) * u128::from(rhs[lane] & MASK52);
            let addend = product as u64 & MASK52;
            expected[lane] = accumulator[lane].wrapping_add(addend.wrapping_mul(iterations));
        }
    }

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = iterations;
    set_zmm(&mut regs, 1, accumulator);
    set_zmm(&mut regs, 2, lhs);
    set_zmm(&mut regs, 3, rhs);
    regs.k[4] = mask;
    vcpu.set_regs(&regs).unwrap();

    assert!(
        vcpu.jit_try_block().expect("jit masked VPMADD52LUQ loop"),
        "a register-only masked VPMADD52LUQ loop must enter the native tier"
    );
    let after_jit = vcpu.get_regs().unwrap();
    assert_eq!(after_jit.rcx & 0xffff_ffff, 0, "native loop drained");
    assert_eq!(get_zmm(&after_jit, 1), expected, "accumulator write-back");
    assert_eq!(get_zmm(&after_jit, 2), lhs, "first source survived");
    assert_eq!(get_zmm(&after_jit, 3), rhs, "second source survived");
    assert_eq!(after_jit.k[4], mask, "source opmask survived");

    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_masked_vdpbf16ps_jits_with_exact_finite_products() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512bf16")
    {
        return;
    }

    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf2, 0x6e, 0xcc, 0x52, 0xcb]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf6]);
    code.push(0xf4);

    let mut lhs = [0u64; 8];
    let mut rhs = [0u64; 8];
    for lane in 0..32 {
        let lhs_bf16 = if lane % 2 == 0 { 0x3f80u64 } else { 0x4000u64 };
        let rhs_bf16 = if lane % 2 == 0 { 0x4040u64 } else { 0x4080u64 };
        lhs[lane / 4] |= lhs_bf16 << ((lane % 4) * 16);
        rhs[lane / 4] |= rhs_bf16 << ((lane % 4) * 16);
    }
    let accumulator = [0x4000_0000_3f80_0000u64; 8]; // lanes alternate 1.0, 2.0
    let mask = 0xa55au64;
    let iterations = 200u64;
    let mut expected = [0u64; 8];
    for lane in 0..16 {
        let value = if ((mask >> lane) & 1) != 0 {
            let initial = if lane % 2 == 0 { 1.0f32 } else { 2.0f32 };
            initial + iterations as f32 * 11.0
        } else {
            0.0
        };
        expected[lane / 2] |= u64::from(value.to_bits()) << ((lane % 2) * 32);
    }

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = iterations;
    set_zmm(&mut regs, 1, accumulator);
    set_zmm(&mut regs, 2, lhs);
    set_zmm(&mut regs, 3, rhs);
    regs.k[4] = mask;
    vcpu.set_regs(&regs).unwrap();

    assert!(
        vcpu.jit_try_block().expect("jit masked VDPBF16PS loop"),
        "a register-only masked VDPBF16PS loop must enter the native tier"
    );
    let after_jit = vcpu.get_regs().unwrap();
    assert_eq!(after_jit.rcx & 0xffff_ffff, 0);
    assert_eq!(get_zmm(&after_jit, 1), expected);
    assert_eq!(get_zmm(&after_jit, 2), lhs);
    assert_eq!(get_zmm(&after_jit, 3), rhs);
    assert_eq!(after_jit.k[4], mask);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_masked_vcvtneps2bf16_jits_with_half_width_result() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512bf16")
    {
        return;
    }

    // loop: vcvtneps2bf16 %zmm2,%ymm1{%k4}{z}
    //       dec ecx
    //       jnz loop
    // hlt
    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf2, 0x7e, 0xcc, 0x72, 0xca]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf6]);
    code.push(0xf4);

    let source: [u64; 8] = std::array::from_fn(|word| {
        let low = (word * 2 + 1) as f32;
        let high = (word * 2 + 2) as f32;
        u64::from(low.to_bits()) | (u64::from(high.to_bits()) << 32)
    });
    let mask = 0xa55au64;
    let mut expected = [0u64; 8];
    for lane in 0..16 {
        if ((mask >> lane) & 1) != 0 {
            let value = (lane + 1) as f32;
            let bf16 = u64::from((value.to_bits() >> 16) as u16);
            expected[lane / 4] |= bf16 << ((lane % 4) * 16);
        }
    }

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 1, [u64::MAX; 8]);
    set_zmm(&mut regs, 2, source);
    regs.k[4] = mask;
    vcpu.set_regs(&regs).unwrap();

    assert!(
        vcpu.jit_try_block().expect("jit masked VCVTNEPS2BF16 loop"),
        "a register-only masked VCVTNEPS2BF16 loop must enter the native tier"
    );
    let after_jit = vcpu.get_regs().unwrap();
    assert_eq!(after_jit.rcx & 0xffff_ffff, 0);
    assert_eq!(get_zmm(&after_jit, 1), expected);
    assert_eq!(get_zmm(&after_jit, 2), source);
    assert_eq!(after_jit.k[4], mask);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_masked_vaddph_jits_with_direct_fp16_mask_semantics() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512fp16")
    {
        return;
    }

    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf5, 0x6c, 0xcc, 0x58, 0xcb]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf6]);
    code.push(0xf4);

    let lhs = [0x3c00_3c00_3c00_3c00u64; 8]; // 1.0 in every FP16 lane
    let rhs = [0x4000_4000_4000_4000u64; 8]; // 2.0 in every FP16 lane
    let mask = 0xa55a_a55au64;
    let mut expected = [0u64; 8];
    for lane in 0..32 {
        if ((mask >> lane) & 1) != 0 {
            expected[lane / 4] |= 0x4200u64 << ((lane % 4) * 16); // 3.0
        }
    }

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 1, [u64::MAX; 8]);
    set_zmm(&mut regs, 2, lhs);
    set_zmm(&mut regs, 3, rhs);
    regs.k[4] = mask;
    vcpu.set_regs(&regs).unwrap();

    assert!(
        vcpu.jit_try_block().expect("jit masked VADDPH loop"),
        "a register-only masked VADDPH loop must enter the native tier"
    );
    let after_jit = vcpu.get_regs().unwrap();
    assert_eq!(after_jit.rcx & 0xffff_ffff, 0);
    assert_eq!(get_zmm(&after_jit, 1), expected);
    assert_eq!(get_zmm(&after_jit, 2), lhs);
    assert_eq!(get_zmm(&after_jit, 3), rhs);
    assert_eq!(after_jit.k[4], mask);
    run_to_hlt(&mut vcpu);
}

#[test]
fn vector_region_with_mmu_load_preserves_complete_zmm_and_k_state() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    // loop: mov eax,[rdi] ; vprold $7,zmm2,zmm1{k4}{z} ; dec ecx ; jnz loop
    let mut code = Vec::new();
    code.extend_from_slice(&[0x8b, 0x07]);
    code.extend_from_slice(&[0x62, 0xf1, 0x75, 0xcc, 0x72, 0xca, 0x07]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf3]);
    code.push(0xf4);
    let data_offset = code.len() as u64;
    let loaded = 0x89ab_cdefu32;
    code.extend_from_slice(&loaded.to_le_bytes());

    let sentinels: [[u64; 8]; 32] = std::array::from_fn(|reg| {
        std::array::from_fn(|word| {
            0x0101_0101_0101_0101u64
                .wrapping_mul((reg as u64 + 1) * 17)
                .wrapping_add(word as u64 * 0x1111_1111_1111_1111)
        })
    });
    let masks: [u64; 8] =
        std::array::from_fn(|index| 0x1111_1111_1111_1111u64.wrapping_mul(index as u64 + 1));
    let mask = masks[4];
    let mut expected = [0u64; 8];
    for lane in 0..16 {
        let input = (sentinels[2][lane / 2] >> ((lane % 2) * 32)) as u32;
        let output = if ((mask >> lane) & 1) != 0 {
            input.rotate_left(7)
        } else {
            0
        };
        expected[lane / 2] |= u64::from(output) << ((lane % 2) * 32);
    }

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    regs.rdi = LOAD_ADDR + data_offset;
    for (index, value) in sentinels.iter().copied().enumerate() {
        set_zmm(&mut regs, index, value);
    }
    regs.k = masks;
    vcpu.set_regs(&regs).unwrap();

    assert!(
        vcpu.jit_try_block().expect("jit vector plus MMU-load loop"),
        "a vector region with a scalar MMU load must enter the native tier"
    );
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xffff_ffff, 0);
    assert_eq!(after.rax & 0xffff_ffff, u64::from(loaded));
    for (index, sentinel) in sentinels.iter().enumerate() {
        assert_eq!(
            get_zmm(&after, index),
            if index == 1 { expected } else { *sentinel },
            "ZMM{index} changed across the MMU helper"
        );
    }
    assert_eq!(after.k, masks, "K registers changed across the MMU helper");
    run_to_hlt(&mut vcpu);
}

#[test]
fn vector_region_mmu_fault_preserves_complete_zmm_and_k_state() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    // Keep a syntactic back-edge so the HLT is a separate frontier block and
    // the pre-fault body is a native region. A straight-line load/vector/HLT
    // block is itself the frontier and is correctly ineligible for promotion.
    let code = [
        0x8b, 0x07, // mov eax,[rdi] -- faults
        0x62, 0xf1, 0x75, 0xcc, 0x72, 0xca, 0x07, // vprold (must not execute)
        0xff, 0xc9, // dec ecx
        0x75, 0xf3, // jnz loop
        0xf4,
    ];
    let sentinels: [[u64; 8]; 32] = std::array::from_fn(|reg| {
        std::array::from_fn(|word| {
            0xf0e1_d2c3_b4a5_9687u64
                .wrapping_add(reg as u64 * 0x0101_0101_0101_0101)
                .wrapping_add(word as u64)
        })
    });
    let masks: [u64; 8] =
        std::array::from_fn(|index| 0xfedc_ba98_7654_3210u64.rotate_left(index as u32 * 7));

    let mut vcpu = make_vcpu(&code);
    vcpu.set_jit_mem(true);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 1;
    regs.rdi = MEM_SIZE + 0x1000;
    for (index, value) in sentinels.iter().copied().enumerate() {
        set_zmm(&mut regs, index, value);
    }
    regs.k = masks;
    vcpu.set_regs(&regs).unwrap();

    assert!(
        vcpu.jit_try_block()
            .expect("jit faulting vector/MMU region"),
        "the mixed region must compile before its MMU access faults"
    );
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rip, LOAD_ADDR, "fault must restart at the load");
    for (index, sentinel) in sentinels.iter().enumerate() {
        assert_eq!(
            get_zmm(&after, index),
            *sentinel,
            "fault changed ZMM{index}"
        );
    }
    assert_eq!(after.k, masks, "fault changed K registers");
}

#[test]
fn vector_region_mmu_path_can_be_explicitly_disabled() {
    let code = [
        0x8b, 0x07, // mov eax,[rdi]
        0x62, 0xf1, 0x75, 0x48, 0x72, 0xca, 0x01, // vprold
        0xf4,
    ];
    let mut vcpu = make_vcpu(&code);
    vcpu.set_jit_mem(false);
    assert!(
        !vcpu.jit_try_block().expect("probe disabled MMU JIT"),
        "explicitly disabling memory JIT must retain interpreter fallback"
    );
}

#[test]
fn vector_region_callout_round_trips_callee_vector_mutations() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        return;
    }

    // caller loop:
    //   vprold $1,zmm2,zmm1
    //   call callee
    //   dec ecx
    //   jnz loop
    //   hlt
    // callee: vprold $7,zmm3,zmm2{k4}{z}; ret
    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf1, 0x75, 0x48, 0x72, 0xca, 0x01]);
    code.extend_from_slice(&[0xe8, 0x05, 0x00, 0x00, 0x00]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf0]);
    code.push(0xf4);
    code.extend_from_slice(&[0x62, 0xf1, 0x6d, 0xcc, 0x72, 0xcb, 0x07, 0xc3]);

    let sentinels: [[u64; 8]; 32] = std::array::from_fn(|reg| {
        std::array::from_fn(|word| {
            0x1020_3040_5060_7080u64
                .wrapping_add(reg as u64 * 0x0101_0101_0101_0101)
                .wrapping_add(word as u64 * 0x0011_0011_0011_0011)
        })
    });
    let masks: [u64; 8] =
        std::array::from_fn(|index| 0x5aa5_9669_a55a_6996u64.rotate_left(index as u32 * 5));
    let mut callee_result = [0u64; 8];
    for lane in 0..16 {
        let input = (sentinels[3][lane / 2] >> ((lane % 2) * 32)) as u32;
        let output = if ((masks[4] >> lane) & 1) != 0 {
            input.rotate_left(7)
        } else {
            0
        };
        callee_result[lane / 2] |= u64::from(output) << ((lane % 2) * 32);
    }
    let mut caller_result = [0u64; 8];
    for lane in 0..16 {
        let input = (callee_result[lane / 2] >> ((lane % 2) * 32)) as u32;
        caller_result[lane / 2] |= u64::from(input.rotate_left(1)) << ((lane % 2) * 32);
    }

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    let initial_rsp = regs.rsp;
    regs.rcx = 200;
    for (index, value) in sentinels.iter().copied().enumerate() {
        set_zmm(&mut regs, index, value);
    }
    regs.k = masks;
    vcpu.set_regs(&regs).unwrap();

    assert!(
        vcpu.jit_try_block().expect("jit vector callout loop"),
        "a vector caller with an interpreted vector callee must enter the native tier"
    );
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xffff_ffff, 0);
    assert_eq!(
        after.rsp, initial_rsp,
        "callout did not balance the guest stack"
    );
    for (index, sentinel) in sentinels.iter().enumerate() {
        let expected = match index {
            1 => caller_result,
            2 => callee_result,
            _ => *sentinel,
        };
        assert_eq!(
            get_zmm(&after, index),
            expected,
            "callout changed ZMM{index}"
        );
    }
    assert_eq!(after.k, masks);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_masked_vpconflictd_jits_with_lower_lane_conflict_masks() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512cd")
    {
        return;
    }

    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf2, 0x7d, 0xcc, 0xc4, 0xca]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf6]);
    code.push(0xf4);

    let lanes = [1u32, 2, 1, 1, 3, 2, 4, 3, 1, 4, 4, 5, 2, 5, 1, 3];
    let mut source = [0u64; 8];
    for (lane, value) in lanes.iter().copied().enumerate() {
        source[lane / 2] |= u64::from(value) << ((lane % 2) * 32);
    }
    let mask = 0xa55au64;
    let mut expected = [0u64; 8];
    for lane in 0..16 {
        if ((mask >> lane) & 1) == 0 {
            continue;
        }
        let mut conflicts = 0u32;
        for previous in 0..lane {
            if lanes[previous] == lanes[lane] {
                conflicts |= 1u32 << previous;
            }
        }
        expected[lane / 2] |= u64::from(conflicts) << ((lane % 2) * 32);
    }

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 1, [u64::MAX; 8]);
    set_zmm(&mut regs, 2, source);
    regs.k[4] = mask;
    vcpu.set_regs(&regs).unwrap();

    assert!(vcpu.jit_try_block().expect("jit masked VPCONFLICTD loop"));
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xffff_ffff, 0);
    assert_eq!(get_zmm(&after, 1), expected);
    assert_eq!(get_zmm(&after, 2), source);
    assert_eq!(after.k[4], mask);
    run_to_hlt(&mut vcpu);
}

#[test]
fn hot_masked_vplzcntd_jits_with_direct_mask_semantics() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512cd")
    {
        return;
    }

    // loop: vplzcntd %zmm2,%zmm1{%k4}{z}
    //       dec ecx
    //       jnz loop
    // hlt
    let mut code = Vec::new();
    code.extend_from_slice(&[0x62, 0xf2, 0x7d, 0xcc, 0x44, 0xca]);
    code.extend_from_slice(&[0xff, 0xc9]);
    code.extend_from_slice(&[0x75, 0xf6]);
    code.push(0xf4);

    let inputs = [
        0u32,
        1,
        2,
        3,
        0x0000_8000,
        0x0001_0000,
        0x00ff_0000,
        0x0100_0000,
        0x4000_0000,
        0x8000_0000,
        0xffff_ffff,
        7,
        8,
        0x0000_00ff,
        0x0000_0100,
        0x7fff_ffff,
    ];
    let mut source = [0u64; 8];
    for (lane, input) in inputs.iter().copied().enumerate() {
        source[lane / 2] |= u64::from(input) << ((lane % 2) * 32);
    }
    let mask = 0xa55au64;
    let mut expected = [0u64; 8];
    for (lane, input) in inputs.iter().copied().enumerate() {
        let output = if ((mask >> lane) & 1) != 0 {
            input.leading_zeros()
        } else {
            0
        };
        expected[lane / 2] |= u64::from(output) << ((lane % 2) * 32);
    }

    let mut vcpu = make_vcpu(&code);
    let mut regs = vcpu.get_regs().unwrap();
    regs.rcx = 200;
    set_zmm(&mut regs, 1, [u64::MAX; 8]);
    set_zmm(&mut regs, 2, source);
    regs.k[4] = mask;
    vcpu.set_regs(&regs).unwrap();

    assert!(vcpu.jit_try_block().expect("jit masked VPLZCNTD loop"));
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rcx & 0xffff_ffff, 0);
    assert_eq!(get_zmm(&after, 1), expected);
    assert_eq!(get_zmm(&after, 2), source);
    assert_eq!(after.k[4], mask);
    run_to_hlt(&mut vcpu);
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
