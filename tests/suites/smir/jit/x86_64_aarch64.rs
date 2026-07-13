//! End-to-end x86 VCPU → SMIR → native AArch64 JIT regressions.
//!
//! These tests execute the production `X86_64Vcpu::jit_try_block` path on an
//! AArch64 host and compare the resulting architectural state with the x86
//! interpreter. A conditional backedge prevents optimizer block merging while
//! each seeded condition takes the forward `hlt` frontier after one iteration.
#![cfg(all(feature = "smir-jit", target_arch = "aarch64"))]

use std::sync::Arc;

use rax::isa::x86_64::X86_64Vcpu;
use rax::vm::vcpu::{Registers, SystemRegisters, VCpu, VcpuExit};
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap, GuestRegionMmap, MmapRegion};

const LOAD_ADDR: u64 = 0x10_0000;
const MEM_SIZE: u64 = 16 * 1024 * 1024;
const STATUS: u64 = (1 << 0) | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 11);

fn make_vcpu_code(code: &[u8]) -> X86_64Vcpu {
    let region = MmapRegion::new(MEM_SIZE as usize).unwrap();
    let guest_region = GuestRegionMmap::new(region, GuestAddress(0)).unwrap();
    let memory = Arc::new(GuestMemoryMmap::from_regions(vec![guest_region]).unwrap());
    memory.write_slice(code, GuestAddress(LOAD_ADDR)).unwrap();

    let mut regs = Registers {
        rip: LOAD_ADDR,
        rsp: 0x11_0000,
        rflags: 0x2,
        ..Default::default()
    };
    // Exercise preservation of mapped GPRs that are not operands.
    regs.rsi = 0x0606_0606_0606_0606;
    regs.r15 = 0x1515_1515_1515_1515;

    let mut sregs = SystemRegisters::default();
    sregs.cr0 = 0x21;
    sregs.cr4 = 0x20;
    sregs.efer = 0x500;
    sregs.cs.limit = u32::MAX;
    sregs.cs.selector = 0x8;
    sregs.cs.type_ = 0xB;
    sregs.cs.present = true;
    sregs.cs.s = true;
    sregs.cs.l = true;
    sregs.cs.g = true;
    sregs.ds.limit = u32::MAX;
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
    for _ in 0..2048 {
        match vcpu.step() {
            Ok(Some(VcpuExit::Hlt)) => return,
            Ok(Some(exit)) => panic!("unexpected x86 VCPU exit: {exit:?}"),
            Ok(None) => {}
            Err(error) => panic!("x86 interpreter error: {error:?}"),
        }
    }
    panic!("x86 program did not reach HLT");
}

fn assert_mapped_state_eq(actual: &Registers, expected: &Registers, label: &str) {
    let actual_gprs = [
        actual.rax, actual.rcx, actual.rdx, actual.rbx, actual.rsp, actual.rbp, actual.rsi,
        actual.rdi, actual.r8, actual.r9, actual.r10, actual.r11, actual.r12, actual.r13,
        actual.r14, actual.r15,
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
    ];
    assert_eq!(actual_gprs, expected_gprs, "{label}: legacy GPR file");
    assert_eq!(actual.rflags, expected.rflags, "{label}: complete RFLAGS");
    assert_eq!(actual.rip, expected.rip, "{label}: RIP");
}

#[test]
fn x86_adcx_adox_execute_natively_and_bridge_both_flag_chains() {
    // adcx rax,rbx; adox rcx,rdx; jnz start; hlt. ADX preserves ZF; every
    // case seeds ZF=1 so the syntactic backedge is not taken at runtime.
    let code = [
        0x66, 0x48, 0x0F, 0x38, 0xF6, 0xC3, // ADCX rax,rbx
        0xF3, 0x48, 0x0F, 0x38, 0xF6, 0xCA, // ADOX rcx,rdx
        0x75, 0xF2, // JNZ start (not taken because ZF=1)
        0xF4,
    ];

    for (label, rax, rbx, rcx, rdx, rflags, expected_rax, expected_rcx, expected_status) in [
        ("chains clear", 5, 3, 7, 1, 0xCD7, 9, 9, 0x0D4),
        (
            "chains carry out",
            u64::MAX,
            0,
            u64::MAX,
            0,
            0xCD7,
            0,
            0,
            0x8D5,
        ),
        ("chains start clear", 5, 3, 7, 1, 0x42, 8, 8, 0x40),
    ] {
        let setup = |vcpu: &mut X86_64Vcpu| {
            let mut regs = vcpu.get_regs().unwrap();
            (regs.rax, regs.rbx, regs.rcx, regs.rdx) = (rax, rbx, rcx, rdx);
            regs.rflags = rflags;
            vcpu.set_regs(&regs).unwrap();
        };

        let mut interpreter = make_vcpu_code(&code);
        setup(&mut interpreter);
        run_to_hlt(&mut interpreter);
        let expected = interpreter.get_regs().unwrap();

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{label}: jit_try_block: {error:?}")),
            "{label}: register-only ADX block must enter the AArch64 native tier"
        );
        run_to_hlt(&mut jit);
        let actual = jit.get_regs().unwrap();

        assert_mapped_state_eq(&actual, &expected, label);
        assert_eq!(actual.rax, expected_rax, "{label}: ADCX result");
        assert_eq!(actual.rcx, expected_rcx, "{label}: ADOX result");
        assert_eq!(actual.rflags & STATUS, expected_status, "{label}: status");
    }
}

#[test]
fn x86_blsi_executes_natively_with_exact_defined_and_preserved_flags() {
    for source in [0, 1, 0x18, u64::MAX] {
        // blsi rax,rax; jcc start; hlt. Select the backedge condition so it is
        // false for this source: JNZ for zero (ZF=1), JZ otherwise (ZF=0).
        let branch = if source == 0 { 0x75 } else { 0x74 };
        let code = [0xC4, 0xE2, 0xF8, 0xF3, 0xD8, branch, 0xF9, 0xF4];
        let setup = |vcpu: &mut X86_64Vcpu| {
            let mut regs = vcpu.get_regs().unwrap();
            regs.rax = source;
            regs.rflags = 0xCD7;
            vcpu.set_regs(&regs).unwrap();
        };

        let mut interpreter = make_vcpu_code(&code);
        setup(&mut interpreter);
        run_to_hlt(&mut interpreter);
        let expected = interpreter.get_regs().unwrap();

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("BLSI {source:#x}: jit_try_block: {error:?}")),
            "BLSI {source:#x}: block must enter the AArch64 native tier"
        );
        run_to_hlt(&mut jit);
        let actual = jit.get_regs().unwrap();

        assert_mapped_state_eq(&actual, &expected, &format!("BLSI {source:#x}"));
        assert_eq!(actual.rax, source & source.wrapping_neg());
        assert_eq!(actual.rflags & 1, u64::from(source != 0), "BLSI CF");
        assert_eq!(
            actual.rflags & (1 << 6),
            u64::from(source == 0) << 6,
            "BLSI ZF"
        );
        assert_eq!(actual.rflags & ((1 << 7) | (1 << 11)), 0, "BLSI SF/OF");
        assert_eq!(
            actual.rflags & ((1 << 2) | (1 << 4)),
            0x14,
            "BLSI PF/AF bridge preservation"
        );
    }
}

#[test]
fn x86_bit_test_family_executes_natively_with_exact_width_and_cf_semantics() {
    // bts eax,ecx; btr rdx,rbx; btc r8,63; bt r9,r10; jnz start; hlt.
    // BT preserves seeded ZF=1, so the syntactic backedge is not taken.
    // W32 proves architectural zero-extension; W64 exercises both register and
    // immediate indexes plus index masking. PF/AF and all non-CF flags survive.
    let code = [
        0x0F, 0xAB, 0xC8, // BTS eax,ecx
        0x48, 0x0F, 0xB3, 0xDA, // BTR rdx,rbx
        0x49, 0x0F, 0xBA, 0xF8, 0x3F, // BTC r8,63
        0x4D, 0x0F, 0xA3, 0xD1, // BT r9,r10
        0x75, 0xEE, // JNZ start (not taken: ZF=1)
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xFFFF_FFFF_0000_0001;
        regs.rcx = 33; // W32 index masks to bit 1.
        regs.rdx = u64::MAX;
        regs.rbx = 68; // W64 index masks to bit 4.
        regs.r8 = 0x8000_0000_0000_0001;
        regs.r9 = 1 << 7;
        regs.r10 = 71; // W64 index masks to bit 7.
        regs.rflags = 0xCD6;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block().expect("bit-test family JIT attempt"),
        "32/64-bit register-only bit-test block must enter the AArch64 tier"
    );
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, "BT/BTS/BTR/BTC");
    assert_eq!(actual.rax, 3, "BTS r32 must set and zero-extend");
    assert_eq!(actual.rdx, u64::MAX & !(1 << 4), "BTR r64");
    assert_eq!(actual.r8, 1, "BTC r64 immediate index");
    assert_eq!(actual.rflags & STATUS, 0x8D5, "only CF changes");
}

#[test]
fn x86_bt_and_bit_update_w16_execute_natively_with_partial_register_merge() {
    // BT has no GPR destination, so its W16 form is exact under the identity
    // bridge. BT preserves seeded ZF=1, so JNZ is not taken.
    let bt = [0x66, 0x0F, 0xA3, 0xC8, 0x75, 0xFA, 0xF4]; // BT ax,cx
    let setup_bt = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 1 << 15;
        regs.rcx = 31; // W16 index masks to bit 15.
        regs.rflags = 0xCD6;
        vcpu.set_regs(&regs).unwrap();
    };
    let mut interpreter = make_vcpu_code(&bt);
    setup_bt(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&bt);
    setup_bt(&mut jit);
    assert!(jit.jit_try_block().expect("BT W16 JIT attempt"));
    run_to_hlt(&mut jit);
    assert_mapped_state_eq(&jit.get_regs().unwrap(), &expected, "BT W16");

    // BTS ax,cx merges the updated low word into RAX and preserves bits 16-63.
    let bts = [0x66, 0x0F, 0xAB, 0xC8, 0x75, 0xFA, 0xF4];
    let setup_bts = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xABCD_EF01_2345_0001;
        regs.rcx = 17; // W16 index masks to bit 1.
        regs.rflags = 0xCD6;
        vcpu.set_regs(&regs).unwrap();
    };
    let mut interpreter = make_vcpu_code(&bts);
    setup_bts(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&bts);
    setup_bts(&mut jit);
    assert!(jit.jit_try_block().expect("BTS W16 eligibility"));
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();
    assert_mapped_state_eq(&actual, &expected, "BTS W16 partial merge");
    assert_eq!(actual.rax, 0xABCD_EF01_2345_0003);
}

#[test]
fn x86_clc_cmc_feed_native_adcx_without_clobbering_other_flags() {
    // CLC; CMC creates CF=1, which ADCX consumes. ZF remains set, making the
    // JNZ backedge not taken. The final carry is exposed through RFLAGS.
    let code = [
        0xF8, // CLC
        0xF5, // CMC
        0x66, 0x48, 0x0F, 0x38, 0xF6, 0xC3, // ADCX rax,rbx
        0x75, 0xF6, // JNZ start
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = u64::MAX;
        regs.rbx = 0;
        regs.rflags = 0xCD6;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(jit.jit_try_block().expect("CLC/CMC/ADCX JIT attempt"));
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, "CLC/CMC/ADCX");
    assert_eq!(actual.rax, 0);
    assert_eq!(
        actual.rflags & STATUS,
        0x8D5,
        "CF set; other status preserved"
    );
}

#[test]
fn x86_subword_mov_and_setcc_execute_natively_with_partial_register_merges() {
    // mov ax,cx; mov dl,0xab; setz bl; cmovz si,di; jnz start; hlt. These
    // operations preserve all flags; seeded ZF=1 selects SETZ/CMOVZ and makes
    // the backedge false.
    let code = [
        0x66, 0x89, 0xC8, // MOV ax,cx
        0xB2, 0xAB, // MOV dl,0xab
        0x0F, 0x94, 0xC3, // SETZ bl
        0x66, 0x0F, 0x44, 0xF7, // CMOVZ si,di
        0x75, 0xF2, // JNZ start
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xAAAA_BBBB_CCCC_DDDD;
        regs.rcx = 0x1111_2222_3333_5678;
        regs.rdx = 0xDEAD_BEEF_1234_5600;
        regs.rbx = 0xBBBB_CCCC_DDDD_EEFF;
        regs.rsi = 0x6666_7777_8888_9999;
        regs.rdi = 0x1111_2222_3333_4444;
        regs.rflags = 0xCD6;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(jit.jit_try_block().expect("subword MOV/SETcc JIT attempt"));
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, "subword MOV/SETcc");
    assert_eq!(actual.rax, 0xAAAA_BBBB_CCCC_5678);
    assert_eq!(actual.rdx, 0xDEAD_BEEF_1234_56AB);
    assert_eq!(actual.rbx, 0xBBBB_CCCC_DDDD_EE01);
    assert_eq!(actual.rsi, 0x6666_7777_8888_4444);
}

#[test]
fn x86_w16_movx_and_cbw_execute_natively_with_partial_register_merges() {
    // movzx dx,cl; movsx si,bl; cbw; movzx bx,dil; jnz start; hlt. Each
    // instruction replaces only the destination's low word. The REX-prefixed
    // DIL source also proves it is not decoded as the legacy BH byte lane.
    // Seeded ZF=1 is preserved, so the syntactic backedge is not taken.
    let code = [
        0x66, 0x0F, 0xB6, 0xD1, // MOVZX dx,cl
        0x66, 0x0F, 0xBE, 0xF3, // MOVSX si,bl
        0x66, 0x98, // CBW
        0x66, 0x40, 0x0F, 0xB6, 0xDF, // MOVZX bx,dil
        0x75, 0xEF, // JNZ start
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xAAAA_BBBB_CCCC_0081;
        regs.rcx = 0x1111_2222_3333_44AB;
        regs.rdx = 0xDEAD_BEEF_1234_5678;
        regs.rbx = 0xBBBB_CCCC_DDDD_FF80;
        regs.rsi = 0x6666_7777_8888_9999;
        regs.rdi = 0x1111_2222_3333_447E;
        regs.rflags = 0xCD6;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block().expect("W16 MOVX/CBW JIT attempt"),
        "W16 MOVSX/MOVZX and CBW must enter the AArch64 tier"
    );
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, "W16 MOVX/CBW");
    assert_eq!(actual.rax, 0xAAAA_BBBB_CCCC_FF81, "CBW alias merge");
    assert_eq!(actual.rcx, 0x1111_2222_3333_44AB, "MOVZX source");
    assert_eq!(actual.rdx, 0xDEAD_BEEF_1234_00AB, "MOVZX dx,cl");
    assert_eq!(actual.rbx, 0xBBBB_CCCC_DDDD_007E, "REX MOVZX bx,dil");
    assert_eq!(actual.rsi, 0x6666_7777_8888_FF80, "MOVSX si,bl");
    assert_eq!(actual.rdi, 0x1111_2222_3333_447E, "REX byte source");
    assert_eq!(actual.rflags, 0xCD6, "extensions preserve RFLAGS");
}

#[test]
fn x86_legacy_high_byte_setcc_remains_interpreter_only() {
    // SETZ AH lifts through a virtual byte and a high-lane merge. The identity
    // bridge has no AH/CH/DH/BH lane mapping, so the block must fail closed.
    let code = [0x0F, 0x94, 0xC4, 0x75, 0xFB, 0xF4]; // SETZ ah; JNZ start; HLT
    let mut vcpu = make_vcpu_code(&code);
    let mut before = vcpu.get_regs().unwrap();
    before.rax = 0xAAAA_BBBB_CCCC_DDDD;
    before.rflags = 0xCD6;
    vcpu.set_regs(&before).unwrap();

    assert!(!vcpu.jit_try_block().expect("high-byte SETcc eligibility"));
    assert_mapped_state_eq(
        &vcpu.get_regs().unwrap(),
        &before,
        "SETZ AH must not execute natively",
    );
}

#[test]
fn x86_subword_not_and_xchg_execute_natively_with_partial_register_merges() {
    // not ax; not dl; xchg si,di; jnz start; hlt. None modifies flags, so
    // seeded ZF=1 keeps the syntactic backedge untaken.
    let code = [
        0x66, 0xF7, 0xD0, // NOT ax
        0xF6, 0xD2, // NOT dl
        0x66, 0x87, 0xFE, // XCHG si,di
        0x75, 0xF6, // JNZ start
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xAAAA_BBBB_CCCC_00F0;
        regs.rdx = 0xDEAD_BEEF_1234_56A5;
        regs.rsi = 0x6666_7777_8888_9999;
        regs.rdi = 0x1111_2222_3333_4444;
        regs.rflags = 0xCD6;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(jit.jit_try_block().expect("subword NOT/XCHG JIT attempt"));
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, "subword NOT/XCHG");
    assert_eq!(actual.rax, 0xAAAA_BBBB_CCCC_FF0F);
    assert_eq!(actual.rdx, 0xDEAD_BEEF_1234_565A);
    assert_eq!(actual.rsi, 0x6666_7777_8888_4444);
    assert_eq!(actual.rdi, 0x1111_2222_3333_9999);
}

#[test]
fn x86_subword_integer_alu_executes_natively_with_partial_register_merges() {
    // {nf} add ax,cx; {nf} add dl,bl; {nf} sub si,di; {nf} neg r8w;
    // {nf} inc r9b; {nf} dec r10w; {nf} and r11w,r12w;
    // {nf} or r13b,r14b; {nf} xor r15w,r15w; jnz start; hlt.
    // APX NF suppresses every flag update. Seeded ZF=1 therefore keeps the
    // syntactic backedge untaken while the complete input RFLAGS survives.
    let code = [
        0x62, 0xF4, 0x7D, 0x0C, 0x01, 0xC8, // {nf} ADD ax,cx
        0x62, 0xF4, 0x7C, 0x0C, 0x00, 0xDA, // {nf} ADD dl,bl
        0x62, 0xF4, 0x7D, 0x0C, 0x29, 0xFE, // {nf} SUB si,di
        0x62, 0xD4, 0x7D, 0x0C, 0xF7, 0xD8, // {nf} NEG r8w
        0x62, 0xD4, 0x7C, 0x0C, 0xFE, 0xC1, // {nf} INC r9b
        0x62, 0xD4, 0x7D, 0x0C, 0xFF, 0xCA, // {nf} DEC r10w
        0x62, 0x54, 0x7D, 0x0C, 0x21, 0xE3, // {nf} AND r11w,r12w
        0x62, 0x54, 0x7C, 0x0C, 0x08, 0xF5, // {nf} OR r13b,r14b
        0x62, 0x54, 0x7D, 0x0C, 0x31, 0xFF, // {nf} XOR r15w,r15w
        0x75, 0xC8, // JNZ start
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        vcpu.set_apx_enabled(true);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xAAAA_BBBB_CCCC_00FF;
        regs.rcx = 0x1111_2222_3333_0001;
        regs.rdx = 0xDEAD_BEEF_1234_56F0;
        regs.rbx = 0xBBBB_CCCC_DDDD_EE20;
        regs.rsi = 0x6666_7777_8888_1000;
        regs.rdi = 0x1111_2222_3333_0001;
        regs.r8 = 0x8888_7777_6666_0001;
        regs.r9 = 0x9999_8888_7777_667F;
        regs.r10 = 0xAAAA_9999_8888_0000;
        regs.r11 = 0xBBBB_AAAA_9999_F0F0;
        regs.r12 = 0xCCCC_BBBB_AAAA_0FF0;
        regs.r13 = 0xDDDD_CCCC_BBBB_AA0F;
        regs.r14 = 0xEEEE_DDDD_CCCC_BBF0;
        regs.r15 = 0xFFFF_EEEE_DDDD_7777;
        regs.rflags = 0xCD7;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("subword integer ALU JIT attempt"),
        "representable low-byte/word ALU block must enter the AArch64 tier"
    );
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, "subword integer ALU");
    assert_eq!(actual.rax, 0xAAAA_BBBB_CCCC_0100);
    assert_eq!(actual.rdx, 0xDEAD_BEEF_1234_5610);
    assert_eq!(actual.rsi, 0x6666_7777_8888_0FFF);
    assert_eq!(actual.r8, 0x8888_7777_6666_FFFF);
    assert_eq!(actual.r9, 0x9999_8888_7777_6680);
    assert_eq!(actual.r10, 0xAAAA_9999_8888_FFFF);
    assert_eq!(actual.r11, 0xBBBB_AAAA_9999_00F0);
    assert_eq!(actual.r13, 0xDDDD_CCCC_BBBB_AAFF);
    assert_eq!(actual.r15, 0xFFFF_EEEE_DDDD_0000);
    assert_eq!(
        actual.rflags & STATUS,
        0x8D5,
        "APX NF preserves status flags"
    );
}

#[test]
fn x86_subword_shift_rotate_executes_natively_with_partial_register_merges() {
    // LLVM 23 APX encodings: {nf} SHL ax,3; {nf} SHR dl,2;
    // {nf} SAR r8w,4; {nf} ROR r9b,3; {nf} ROL r10w,5.
    // Every operation preserves RFLAGS, including seeded ZF=1, so the
    // syntactic JNZ backedge remains untaken.
    let code = [
        0x62, 0xF4, 0x7D, 0x0C, 0xC1, 0xE0, 0x03, // {nf} SHL ax,3
        0x62, 0xF4, 0x7C, 0x0C, 0xC0, 0xEA, 0x02, // {nf} SHR dl,2
        0x62, 0xD4, 0x7D, 0x0C, 0xC1, 0xF8, 0x04, // {nf} SAR r8w,4
        0x62, 0xD4, 0x7C, 0x0C, 0xC0, 0xC9, 0x03, // {nf} ROR r9b,3
        0x62, 0xD4, 0x7D, 0x0C, 0xC1, 0xC2, 0x05, // {nf} ROL r10w,5
        0x75, 0xDB, // JNZ start
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        vcpu.set_apx_enabled(true);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xAAAA_BBBB_CCCC_1234;
        regs.rdx = 0xDEAD_BEEF_1234_56F0;
        regs.r8 = 0x8888_7777_6666_8000;
        regs.r9 = 0x9999_8888_7777_6681;
        regs.r10 = 0xAAAA_9999_8888_8001;
        regs.rflags = 0xCD6;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("subword shift/rotate JIT attempt"),
        "APX NF subword shift/rotate block must enter the AArch64 tier"
    );
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, "subword shift/rotate");
    assert_eq!(actual.rax, 0xAAAA_BBBB_CCCC_91A0);
    assert_eq!(actual.rdx, 0xDEAD_BEEF_1234_563C);
    assert_eq!(actual.r8, 0x8888_7777_6666_F800);
    assert_eq!(actual.r9, 0x9999_8888_7777_6630);
    assert_eq!(actual.r10, 0xAAAA_9999_8888_0030);
    assert_eq!(actual.rflags, 0xCD6, "APX NF preserves complete RFLAGS");
}

#[test]
fn x86_subword_rotates_bridge_defined_flags_and_preserve_upper_bits() {
    // rol ax,1; ror dl,1; jnz start; hlt. Rotates define only CF/OF and
    // preserve seeded ZF/PF/AF, so all architecturally defined flag effects are
    // representable by the x86/AArch64 bridge.
    let code = [
        0x66, 0xD1, 0xC0, // ROL ax,1
        0xD0, 0xCA, // ROR dl,1
        0x75, 0xF9, // JNZ start
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xAAAA_BBBB_CCCC_8001;
        regs.rdx = 0xDEAD_BEEF_1234_5681;
        regs.rflags = 0xCD6;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(jit.jit_try_block().expect("subword rotate JIT attempt"));
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, "subword flag-setting rotates");
    assert_eq!(actual.rax, 0xAAAA_BBBB_CCCC_0003);
    assert_eq!(actual.rdx, 0xDEAD_BEEF_1234_56C0);
    assert_eq!(actual.rflags & STATUS, expected.rflags & STATUS);
}

#[test]
fn x86_subword_carry_rotates_execute_natively_with_partial_register_merges() {
    // rcl al,1; rcr cx,1; jnz start; hlt. Both rotates consume and define CF,
    // define OF for count one, and preserve seeded ZF/PF/AF. The final JNZ is
    // therefore not taken while both byte/word upper-register merges execute.
    let code = [
        0xD0, 0xD0, // RCL al,1
        0x66, 0xD1, 0xD9, // RCR cx,1
        0x75, 0xF9, // JNZ start
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xAAAA_BBBB_CCCC_DD81;
        regs.rcx = 0x1111_2222_3333_0001;
        regs.rflags = 0xCD7;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(jit.jit_try_block().expect("subword RCL/RCR JIT attempt"));
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, "subword RCL/RCR");
    assert_eq!(actual.rax, 0xAAAA_BBBB_CCCC_DD03, "RCL al upper merge");
    assert_eq!(actual.rcx, 0x1111_2222_3333_8000, "RCR cx upper merge");
    assert_eq!(actual.rflags & STATUS, expected.rflags & STATUS);
}

#[test]
fn x86_apx_ndd_double_shifts_execute_natively_across_widths_and_aliases() {
    // LLVM 23 encodings: {nf} SHLD r8w,ax,bx,4;
    // {nf} SHRD ecx,eax,ebx,cl; {nf} SHLD rbx,rax,rbx,4.
    // The sequence covers W16 partial-register merge, W32 zero-extension,
    // dst==CL, and dst==fill while preserving every status flag.
    let code = [
        0x62, 0xF4, 0x3D, 0x1C, 0x24, 0xD8, 0x04, 0x62, 0xF4, 0x74, 0x1C, 0xAD, 0xD8, 0x62, 0xF4,
        0xE4, 0x1C, 0x24, 0xD8, 0x04, 0x75,
        0xEA, // JNZ start (not taken because APX NF preserves ZF=1)
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        vcpu.set_apx_enabled(true);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xAAAA_BBBB_CCCC_8123;
        regs.rcx = 0x1111_2222_3333_0005;
        regs.rbx = 0xBBBB_CCCC_DDDD_5AA5;
        regs.r8 = 0x8888_7777_6666_2468;
        regs.rflags = 0xCD6;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("APX NDD double-shift JIT attempt"),
        "APX NF NDD double shifts must enter the AArch64 native tier"
    );
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, "APX NDD double shifts");
    assert_eq!(actual.r8, 0x8888_7777_6666_1235, "W16 upper merge");
    assert_eq!(actual.rcx, 0x2E66_6409, "W32 count alias");
    assert_eq!(actual.rbx, 0xAAAB_BBBC_CCC8_123B, "W64 fill alias");
    assert_eq!(actual.rflags, 0xCD6, "APX NF preserves complete RFLAGS");
}

#[test]
fn x86_apx_nf_w16_destructive_double_shifts_merge_partial_registers() {
    // LLVM 23 encodings: {nf} SHLD ax,bx,4; {nf} SHRD cx,dx,cl.
    // The second operation consumes the original CL while committing only the
    // low 16 destination bits. APX NF preserves the complete RFLAGS snapshot.
    let code = [
        0x62, 0xF4, 0x7D, 0x0C, 0x24, 0xD8, 0x04, 0x62, 0xF4, 0x7D, 0x0C, 0xAD, 0xD1, 0x75, 0xF1,
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        vcpu.set_apx_enabled(true);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xAAAA_BBBB_CCCC_8123;
        regs.rcx = 0x1111_2222_3333_0005;
        regs.rdx = 0xDDDD_EEEE_FFFF_ABCD;
        regs.rbx = 0xBBBB_CCCC_DDDD_5AA5;
        regs.rflags = 0xCD6;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("APX NF W16 destructive double-shift JIT attempt"),
        "APX NF W16 destructive double shifts must enter the AArch64 tier"
    );
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, "APX NF W16 destructive double shifts");
    assert_eq!(actual.rax, 0xAAAA_BBBB_CCCC_1235, "SHLD ax upper merge");
    assert_eq!(actual.rcx, 0x1111_2222_3333_6800, "SHRD cx/CL alias");
    assert_eq!(actual.rflags, 0xCD6, "APX NF preserves complete RFLAGS");
}

#[test]
fn x86_apx_nf_w16_signed_multiply_merges_partial_registers() {
    // LLVM 23 encodings: destructive IMUL r11w,r12w; immediate IMUL
    // r13w,r14w,0x1234; NDD IMUL r12w,r11w,r12w. The last form aliases its
    // destination with the second source. JNZ detects any unintended NF flag
    // mutation before HLT.
    let code = [
        0x62, 0x54, 0x7D, 0x0C, 0xAF, 0xDC, 0x62, 0x54, 0x7D, 0x0C, 0x69, 0xEE, 0x34, 0x12, 0x62,
        0x54, 0x1D, 0x1C, 0xAF, 0xDC, 0x75, 0xEA, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        vcpu.set_apx_enabled(true);
        let mut regs = vcpu.get_regs().unwrap();
        regs.r11 = 0xBBBB_AAAA_9999_FFFE;
        regs.r12 = 0xCCCC_BBBB_AAAA_0003;
        regs.r13 = 0xDDDD_CCCC_BBBB_7777;
        regs.r14 = 0xEEEE_DDDD_CCCC_0002;
        regs.rflags = 0xCD6;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block()
            .expect("APX NF W16 signed-multiply JIT attempt"),
        "APX NF W16 signed multiplies must enter the AArch64 tier"
    );
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, "APX NF W16 signed multiply");
    assert_eq!(actual.r11, 0xBBBB_AAAA_9999_FFFA, "destructive IMUL");
    assert_eq!(actual.r12, 0xCCCC_BBBB_AAAA_FFEE, "NDD source alias");
    assert_eq!(actual.r13, 0xDDDD_CCCC_BBBB_2468, "imm16 IMUL");
    assert_eq!(actual.rflags, 0xCD6, "APX NF preserves complete RFLAGS");
}

#[test]
fn x86_w16_bit_scans_merge_partial_registers_and_only_update_zf() {
    // LLVM 23 encodings: BSR ax,bx; BSF cx,cx. Both sources are nonzero, so
    // ZF clears and JZ falls through. CF/SF/OF and all non-NZCV RFLAGS bits
    // must survive the Specific(ZF) SMIR contract.
    let code = [
        0x66, 0x0F, 0xBD, 0xC3, 0x66, 0x0F, 0xBC, 0xC9, 0x74, 0xF6, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xAAAA_BBBB_CCCC_7777;
        regs.rcx = 0x1111_2222_3333_0100;
        regs.rbx = 0xBBBB_CCCC_DDDD_8000;
        regs.rflags = 0xCD7;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block().expect("W16 bit-scan JIT attempt"),
        "W16 bit scans must enter the AArch64 tier"
    );
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, "W16 bit scans");
    assert_eq!(actual.rax, 0xAAAA_BBBB_CCCC_000F, "BSR ax upper merge");
    assert_eq!(actual.rcx, 0x1111_2222_3333_0008, "BSF cx alias");
    assert_eq!(actual.rbx, 0xBBBB_CCCC_DDDD_8000, "source preserved");
    assert_eq!(actual.rflags, 0xC97, "only ZF clears");
}

#[test]
fn x86_apx_nf_w16_counts_merge_partial_registers_and_preserve_flags() {
    // LLVM 23 encodings: {nf} POPCNT r8w,ax; {nf} LZCNT r9w,r9w;
    // {nf} TZCNT r11w,r10w. JNZ detects any unintended NF flag mutation.
    let code = [
        0x62, 0x74, 0x7D, 0x0C, 0x88, 0xC0, 0x62, 0x54, 0x7D, 0x0C, 0xF5, 0xC9, 0x62, 0x54, 0x7D,
        0x0C, 0xF4, 0xDA, 0x75, 0xEC, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        vcpu.set_apx_enabled(true);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rax = 0xAAAA_BBBB_CCCC_F0F0;
        regs.r8 = 0x8888_7777_6666_7777;
        regs.r9 = 0x9999_8888_7777_0100;
        regs.r10 = 0xAAAA_9999_8888_8000;
        regs.r11 = 0xBBBB_AAAA_9999_7777;
        regs.rflags = 0xCD6;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block().expect("APX NF W16 count JIT attempt"),
        "APX NF W16 counts must enter the AArch64 tier"
    );
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, "APX NF W16 counts");
    assert_eq!(actual.r8, 0x8888_7777_6666_0008, "POPCNT upper merge");
    assert_eq!(actual.r9, 0x9999_8888_7777_0007, "LZCNT alias");
    assert_eq!(actual.r10, 0xAAAA_9999_8888_8000, "TZCNT source");
    assert_eq!(actual.r11, 0xBBBB_AAAA_9999_000F, "TZCNT upper merge");
    assert_eq!(actual.rflags, 0xCD6, "APX NF preserves complete RFLAGS");
}

#[test]
fn x86_w16_tzcnt_lzcnt_merge_cf_zf_and_preserve_other_flags() {
    // LLVM 23 encodings: TZCNT cx,cx; LZCNT si,dx. The final high-bit LZCNT
    // result is zero, so ZF=1/CF=0 and JNZ falls through.
    let code = [
        0x66, 0xF3, 0x0F, 0xBC, 0xC9, 0x66, 0xF3, 0x0F, 0xBD, 0xF2, 0x75, 0xF4, 0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 0x1111_2222_3333_0000;
        regs.rdx = 0xDDDD_EEEE_FFFF_8000;
        regs.rsi = 0x6666_5555_4444_7777;
        regs.rflags = 0xC97;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    assert!(
        jit.jit_try_block().expect("W16 TZCNT/LZCNT JIT attempt"),
        "W16 TZCNT/LZCNT must enter the AArch64 tier"
    );
    run_to_hlt(&mut jit);
    let actual = jit.get_regs().unwrap();

    assert_mapped_state_eq(&actual, &expected, "W16 TZCNT/LZCNT");
    assert_eq!(actual.rcx, 0x1111_2222_3333_0010, "TZCNT alias");
    assert_eq!(actual.rdx, 0xDDDD_EEEE_FFFF_8000, "LZCNT source");
    assert_eq!(actual.rsi, 0x6666_5555_4444_0000, "LZCNT upper merge");
    assert_eq!(actual.rflags, 0xCD6, "only CF/ZF replaced");
}

#[test]
fn x86_full_multiply_sub64_and_shared_mulx_aliases_execute_natively() {
    let cases = [
        (
            "MULX r32 distinct destinations",
            vec![0xC4, 0xE2, 0x73, 0xF6, 0xC3, 0x75, 0xF9, 0xF4],
            false,
        ),
        (
            "MULX r64 shared destination",
            vec![0xC4, 0xE2, 0xF3, 0xF6, 0xCA, 0x75, 0xF9, 0xF4],
            false,
        ),
        (
            "APX NF MUL r16 implicit pair",
            vec![0x62, 0xF4, 0x7D, 0x0C, 0xF7, 0xE1, 0x75, 0xF8, 0xF4],
            true,
        ),
        (
            "APX NF IMUL r16 implicit pair",
            vec![0x62, 0xF4, 0x7D, 0x0C, 0xF7, 0xE9, 0x75, 0xF8, 0xF4],
            true,
        ),
    ];

    for (label, code, apx) in cases {
        let setup = |vcpu: &mut X86_64Vcpu| {
            vcpu.set_apx_enabled(apx);
            let mut regs = vcpu.get_regs().unwrap();
            regs.rflags = 0xCD6;
            match label {
                "MULX r32 distinct destinations" => {
                    regs.rax = 0xAAAA_BBBB_CCCC_DDDD;
                    regs.rcx = 0x1111_2222_3333_4444;
                    regs.rdx = 0xDDDD_EEEE_FFFF_FFFE;
                    regs.rbx = 0xBBBB_CCCC_8000_0003;
                }
                "MULX r64 shared destination" => {
                    regs.rcx = 0x1111_2222_3333_4444;
                    regs.rdx = 0xF000_0000_0000_0003;
                }
                "APX NF MUL r16 implicit pair" => {
                    regs.rax = 0xAAAA_BBBB_CCCC_1234;
                    regs.rdx = 0xDDDD_EEEE_FFFF_5678;
                    regs.rcx = 0x1111_2222_3333_0003;
                }
                "APX NF IMUL r16 implicit pair" => {
                    regs.rax = 0xAAAA_BBBB_CCCC_FFFD;
                    regs.rdx = 0xDDDD_EEEE_FFFF_5678;
                    regs.rcx = 0x1111_2222_3333_0004;
                }
                _ => unreachable!(),
            }
            vcpu.set_regs(&regs).unwrap();
        };

        let mut interpreter = make_vcpu_code(&code);
        setup(&mut interpreter);
        run_to_hlt(&mut interpreter);
        let expected = interpreter.get_regs().unwrap();

        let mut jit = make_vcpu_code(&code);
        setup(&mut jit);
        assert!(
            jit.jit_try_block()
                .unwrap_or_else(|error| panic!("{label}: JIT attempt: {error:?}")),
            "{label}: block must enter the AArch64 tier"
        );
        run_to_hlt(&mut jit);
        let actual = jit.get_regs().unwrap();

        assert_mapped_state_eq(&actual, &expected, label);
        assert_eq!(actual.rflags, 0xCD6, "{label}: flags preserved");
        match label {
            "MULX r32 distinct destinations" => {
                let product = u64::from(0xFFFF_FFFEu32) * u64::from(0x8000_0003u32);
                assert_eq!(actual.rax, product >> 32, "{label}: high EAX");
                assert_eq!(actual.rcx, product & 0xFFFF_FFFF, "{label}: low ECX");
                assert_eq!(actual.rdx, 0xDDDD_EEEE_FFFF_FFFE, "{label}: RDX");
                assert_eq!(actual.rbx, 0xBBBB_CCCC_8000_0003, "{label}: RBX");
            }
            "MULX r64 shared destination" => {
                let source = 0xF000_0000_0000_0003_u128;
                assert_eq!(actual.rcx, ((source * source) >> 64) as u64, "{label}");
            }
            "APX NF MUL r16 implicit pair" => {
                assert_eq!(actual.rax, 0xAAAA_BBBB_CCCC_369C, "{label}: AX");
                assert_eq!(actual.rdx, 0xDDDD_EEEE_FFFF_0000, "{label}: DX");
            }
            "APX NF IMUL r16 implicit pair" => {
                assert_eq!(actual.rax, 0xAAAA_BBBB_CCCC_FFF4, "{label}: AX");
                assert_eq!(actual.rdx, 0xDDDD_EEEE_FFFF_FFFF, "{label}: DX");
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn x86_aarch64_jit_rejects_live_pf_af_definitions_without_execution() {
    // add rax,rbx; jnz start; hlt. ADD's live PF/AF outputs cannot be represented
    // in NZCV, so the architecture-specific gate must retain interpreter fallback.
    // The operands produce zero, making JNZ not-taken if this is ever admitted.
    let code = [0x48, 0x01, 0xD8, 0x75, 0xFB, 0xF4];
    let mut vcpu = make_vcpu_code(&code);
    let mut before = vcpu.get_regs().unwrap();
    before.rax = u64::MAX;
    before.rbx = 1;
    before.rflags = 0xCD7;
    vcpu.set_regs(&before).unwrap();

    assert!(!vcpu.jit_try_block().expect("ineligible ADD block"));
    let after = vcpu.get_regs().unwrap();
    assert_mapped_state_eq(&after, &before, "ineligible ADD must not execute");
}

#[test]
fn x86_aarch64_run_auto_promotes_and_caches_hot_loop() {
    // loop:
    //   {nf} dec rcx       ; counter update without flag side effects
    //   blsi rdx,rcx       ; ZF=1 exactly when the counter reaches zero
    //   jnz loop
    //   hlt
    //
    // 500 iterations cross the production 64-backedge promotion threshold.
    // Auto-promotion lowers the backward edge as an inline native exit, so this
    // also exercises cached region re-entry and edge-exit PC recording.
    let code = [
        0x62, 0xF4, 0xFC, 0x0C, 0xFF, 0xC9, // APX NF DEC rcx
        0xC4, 0xE2, 0xE8, 0xF3, 0xD9, // BLSI rdx,rcx
        0x75, 0xF3, // JNZ loop
        0xF4,
    ];
    let setup = |vcpu: &mut X86_64Vcpu| {
        vcpu.set_apx_enabled(true);
        let mut regs = vcpu.get_regs().unwrap();
        regs.rcx = 500;
        regs.rdx = u64::MAX;
        regs.rflags = 0xCD7;
        vcpu.set_regs(&regs).unwrap();
    };

    let mut interpreter = make_vcpu_code(&code);
    setup(&mut interpreter);
    run_to_hlt(&mut interpreter);
    let expected = interpreter.get_regs().unwrap();

    let mut jit = make_vcpu_code(&code);
    setup(&mut jit);
    for _ in 0..10_000 {
        let _ = jit.run().expect("AArch64 x86 hot-loop run");
        if jit.get_regs().unwrap().rcx == 0 {
            break;
        }
    }
    let actual = jit.get_regs().unwrap();

    assert_eq!(actual.rcx, 0, "hot loop must drain");
    assert_eq!(actual.rdx, expected.rdx, "final BLSI result");
    assert_eq!(actual.rflags, expected.rflags, "complete final RFLAGS");
    assert_eq!(actual.rsi, expected.rsi, "non-operand mapped GPR");
    assert_eq!(actual.r15, expected.r15, "high mapped GPR");
    assert!(
        jit.jit_region_count() >= 1,
        "run() must auto-compile and cache the eligible AArch64-host region"
    );
}
