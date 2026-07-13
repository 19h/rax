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
