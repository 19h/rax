//! Direct/native INVPCID differentials over descriptor and stale translations.

use super::*;
use crate::isa::x86_64::mmu::AccessType;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const PML4: u64 = 0x1000;
const PDPT: u64 = 0x2000;
const PD: u64 = 0x3000;
const PT: u64 = 0x4000;
const OLD_PAGE: u64 = 0x5000;
const NEW_PAGE: u64 = 0x6000;
const CODE_PAGE: u64 = 0x8000;
const DESCRIPTOR_PAGE: u64 = 0xA000;
const TEST_VADDR: u64 = 0x7000;
const DESCRIPTOR_VADDR: u64 = 0x9000;
const PAGE_FLAGS: u64 = 0x7; // Present | writable | user-accessible.

fn paged_vcpu(
    code: &[u8],
    descriptor_low: u64,
    descriptor_linear: u64,
) -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x20000)]).unwrap());
    for (address, entry) in [
        (PML4, PDPT | PAGE_FLAGS),
        (PDPT, PD | PAGE_FLAGS),
        (PD, PT | PAGE_FLAGS),
        (PT, CODE_PAGE | PAGE_FLAGS),
        (PT + (TEST_VADDR >> 12) * 8, OLD_PAGE | PAGE_FLAGS),
        (
            PT + (DESCRIPTOR_VADDR >> 12) * 8,
            DESCRIPTOR_PAGE | PAGE_FLAGS,
        ),
    ] {
        memory
            .write_slice(&entry.to_le_bytes(), GuestAddress(address))
            .unwrap();
    }
    memory.write_slice(code, GuestAddress(CODE_PAGE)).unwrap();
    memory
        .write_slice(&descriptor_low.to_le_bytes(), GuestAddress(DESCRIPTOR_PAGE))
        .unwrap();
    memory
        .write_slice(
            &descriptor_linear.to_le_bytes(),
            GuestAddress(DESCRIPTOR_PAGE + 8),
        )
        .unwrap();

    let mut vcpu = X86_64Vcpu::new(0, memory.clone());
    vcpu.sregs.efer = (1 << 8) | (1 << 10);
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.sregs.cr0 = 0x8005_0033;
    vcpu.sregs.cr3 = PML4;
    vcpu.sregs.cr4 = (1 << 5) | (1 << 17); // PAE | PCIDE.
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0xB000;
    // linux/amd64 user-mode emulation on an Arm host clears an imported AF
    // across pushfq/popfq. Keep every other modeled status/control bit set.
    vcpu.regs.rflags = (0x2 | 0x08D5 | flags::bits::DF) & !flags::bits::AF;
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);
    (vcpu, memory)
}

fn seed_stale_translation(vcpu: &mut X86_64Vcpu, memory: &GuestMemoryMmap) {
    let sregs = vcpu.sregs.clone();
    assert_eq!(
        vcpu.mmu
            .translate(TEST_VADDR, AccessType::Read, &sregs)
            .unwrap(),
        OLD_PAGE
    );
    memory
        .write_slice(
            &(NEW_PAGE | PAGE_FLAGS).to_le_bytes(),
            GuestAddress(PT + (TEST_VADDR >> 12) * 8),
        )
        .unwrap();
}

fn observed_translation(vcpu: &mut X86_64Vcpu) -> u64 {
    let sregs = vcpu.sregs.clone();
    vcpu.mmu
        .translate(TEST_VADDR, AccessType::Read, &sregs)
        .unwrap()
}

fn configure_legacy(vcpu: &mut X86_64Vcpu, invpcid_type: u64) {
    vcpu.regs.rax = invpcid_type;
    vcpu.regs.rbx = DESCRIPTOR_VADDR;
}

#[test]
fn direct_invpcid_reads_exact_descriptor_and_flushes_translation_dependent_state() {
    let code = [0x66, 0x0F, 0x38, 0x82, 0x03, 0xF4];
    let (mut vcpu, memory) = paged_vcpu(&code, 0x123, TEST_VADDR);
    seed_stale_translation(&mut vcpu, &memory);
    configure_legacy(&mut vcpu, 0);
    let initial_flags = vcpu.regs.rflags;
    vcpu.jit_mem_trace = Some(Vec::new());
    vcpu.jit_hot.insert(0x2000, 7);

    assert!(vcpu.step().expect("direct INVPCID").is_none());

    assert_eq!(vcpu.regs.rip, 5);
    assert_eq!(vcpu.regs.rflags, initial_flags);
    assert_eq!(observed_translation(&mut vcpu), NEW_PAGE);
    assert!(vcpu.jit_hot.is_empty());
    assert_eq!(
        vcpu.jit_mem_trace.as_deref(),
        Some(
            &[
                (0, DESCRIPTOR_VADDR, 8, 0x123),
                (0, DESCRIPTOR_VADDR + 8, 8, TEST_VADDR),
            ][..]
        )
    );
}

#[test]
fn native_invpcid_matches_direct_for_legacy_and_apx_and_exits_at_the_handoff() {
    for (name, code, configure, next_pc) in [
        (
            "legacy",
            &[0x66, 0x0F, 0x38, 0x82, 0x03, 0xEB, 0x00, 0xF4][..],
            configure_legacy as fn(&mut X86_64Vcpu, u64),
            5,
        ),
        (
            "apx",
            &[0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01, 0xEB, 0x00, 0xF4],
            |vcpu: &mut X86_64Vcpu, invpcid_type| {
                vcpu.set_apx_enabled(true);
                vcpu.regs.r16 = invpcid_type;
                vcpu.regs.r17 = DESCRIPTOR_VADDR;
            },
            6,
        ),
    ] {
        let (mut vcpu, memory) = paged_vcpu(code, 0x321, 0x0000_7FFF_FFFF_F000);
        seed_stale_translation(&mut vcpu, &memory);
        configure(&mut vcpu, 2);
        let initial_flags = vcpu.regs.rflags;
        let region = Arc::new(
            vcpu.jit_compile_region()
                .expect("compile INVPCID region")
                .expect("INVPCID must be native eligible with MMU helpers"),
        );
        let cache_key = (vcpu.regs.rip, vcpu.jit_mode_tag());
        vcpu.jit_cache.insert(cache_key, Some(region.clone()));
        vcpu.jit_hot.insert(0x2000, 7);
        vcpu.jit_ineligible.insert((0x3000, 0), vec![0xF4]);
        vcpu.jit_ineligible_dirty.insert((0x3000, 0));

        vcpu.jit_run_region_native(&region);

        assert_eq!(vcpu.regs.rip, next_pc, "{name}");
        assert_eq!(vcpu.regs.rflags, initial_flags, "{name}");
        assert_eq!(observed_translation(&mut vcpu), NEW_PAGE, "{name}");
        assert!(vcpu.jit_cache.is_empty(), "{name}");
        assert!(vcpu.jit_hot.is_empty(), "{name}");
        assert!(vcpu.jit_ineligible.is_empty(), "{name}");
        assert!(vcpu.jit_ineligible_dirty.is_empty(), "{name}");
    }
}

#[test]
fn verified_invpcid_replays_the_same_descriptor_trace_and_exact_frontier() {
    let code = [0x66, 0x0F, 0x38, 0x82, 0x03, 0xF4];
    let (mut vcpu, memory) = paged_vcpu(&code, 0xABC, 0x0000_8000_0000_0000);
    seed_stale_translation(&mut vcpu, &memory);
    configure_legacy(&mut vcpu, 1);
    let initial_flags = vcpu.regs.rflags;
    let region = Arc::new(
        vcpu.jit_compile_region()
            .expect("compile verified INVPCID region")
            .expect("INVPCID must be native eligible with MMU helpers"),
    );
    let cache_key = (vcpu.regs.rip, vcpu.jit_mode_tag());
    vcpu.jit_cache.insert(cache_key, Some(region.clone()));

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.regs.rip, 5);
    assert_eq!(vcpu.regs.rflags, initial_flags);
    assert_eq!(observed_translation(&mut vcpu), NEW_PAGE);
    assert!(vcpu.jit_cache.is_empty());
}

#[test]
fn verified_invalid_invpcid_descriptor_matches_noncommitting_interpreter_replay() {
    let code = [0x66, 0x0F, 0x38, 0x82, 0x03, 0xF4];
    let (mut vcpu, memory) = paged_vcpu(&code, 1 << 12, TEST_VADDR);
    seed_stale_translation(&mut vcpu, &memory);
    configure_legacy(&mut vcpu, 0);
    let initial_flags = vcpu.regs.rflags;
    let region = Arc::new(
        vcpu.jit_compile_region()
            .expect("compile invalid-descriptor INVPCID region")
            .expect("dynamic descriptor failure must remain native eligible"),
    );
    let cache_key = (vcpu.regs.rip, vcpu.jit_mode_tag());
    vcpu.jit_cache.insert(cache_key, Some(region.clone()));

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.regs.rip, 0);
    assert_eq!(vcpu.regs.rflags, initial_flags);
    // Verifier state adoption rebuilds the derived MMU cache independently of
    // INVPCID. The native-only fault matrix below is the no-flush oracle; here
    // retention of the compiled region establishes that trace comparison did
    // not diagnose a divergence and invalidate JIT state.
    assert!(vcpu.jit_cache.contains_key(&cache_key));
}

#[test]
fn jit_invpcid_helper_rejects_malformed_abi_and_dynamic_guards_before_invalidation() {
    use crate::smir::lower::runtime::GuestRegs;

    let (mut vcpu, memory) = paged_vcpu(&[0xF4], 0, TEST_VADDR);
    seed_stale_translation(&mut vcpu, &memory);
    vcpu.jit_hot.insert(0x2000, 7);
    let mut state = GuestRegs::default();
    state.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;
    state.cs_l = 1;
    state.cpl = 0;
    state.apx_enabled = 1;
    state.cr4 = (1 << 5) | (1 << 17);

    assert_eq!(
        unsafe { rax_jit_invpcid(std::ptr::null_mut(), DESCRIPTOR_VADDR, 2, 0) },
        0
    );
    assert_eq!(
        unsafe { rax_jit_invpcid(&mut state, DESCRIPTOR_VADDR, 2, 2) },
        0,
        "requires_apx is a Boolean ABI field"
    );
    state.ctx = 0;
    assert_eq!(
        unsafe { rax_jit_invpcid(&mut state, DESCRIPTOR_VADDR, 2, 0) },
        0
    );
    state.ctx = (&mut vcpu as *mut X86_64Vcpu) as u64;
    state.cs_l = 0;
    assert_eq!(
        unsafe { rax_jit_invpcid(&mut state, DESCRIPTOR_VADDR, 2, 0) },
        0
    );
    state.cs_l = 1;
    state.cpl = 3;
    assert_eq!(
        unsafe { rax_jit_invpcid(&mut state, DESCRIPTOR_VADDR, 2, 0) },
        0
    );
    state.cpl = 0;
    state.apx_enabled = 0;
    assert_eq!(
        unsafe { rax_jit_invpcid(&mut state, DESCRIPTOR_VADDR, 2, 1) },
        0
    );
    state.apx_enabled = 1;
    assert_eq!(
        unsafe { rax_jit_invpcid(&mut state, u64::MAX - 7, 2, 0) },
        0,
        "a wrapping 16-byte source must be rejected before memory access"
    );

    assert_eq!(observed_translation(&mut vcpu), OLD_PAGE);
    assert_eq!(vcpu.jit_hot.get(&0x2000), Some(&7));
}

#[test]
fn native_invpcid_dynamic_faults_replay_without_invalidation_or_partial_commit() {
    struct Case {
        name: &'static str,
        code: &'static [u8],
        vector: u8,
        invpcid_type: u64,
        descriptor_low: u64,
        descriptor_linear: u64,
    }

    let cases = [
        Case {
            name: "CPL",
            code: &[0x66, 0x0F, 0x38, 0x82, 0x03, 0xF4],
            vector: 13,
            invpcid_type: 2,
            descriptor_low: 0,
            descriptor_linear: TEST_VADDR,
        },
        Case {
            name: "APX",
            code: &[0x62, 0xEC, 0x7E, 0x08, 0xF2, 0x01, 0xF4],
            vector: 6,
            invpcid_type: 2,
            descriptor_low: 0,
            descriptor_linear: TEST_VADDR,
        },
        Case {
            name: "page-fault",
            code: &[0x66, 0x0F, 0x38, 0x82, 0x03, 0xF4],
            vector: 14,
            invpcid_type: 4,
            descriptor_low: 0,
            descriptor_linear: TEST_VADDR,
        },
        Case {
            name: "cross-page-fault",
            code: &[0x66, 0x0F, 0x38, 0x82, 0x03, 0xF4],
            vector: 14,
            invpcid_type: 4,
            descriptor_low: 0,
            descriptor_linear: TEST_VADDR,
        },
        Case {
            name: "invalid-type",
            code: &[0x66, 0x0F, 0x38, 0x82, 0x03, 0xF4],
            vector: 13,
            invpcid_type: 4,
            descriptor_low: 0,
            descriptor_linear: TEST_VADDR,
        },
        Case {
            name: "reserved-descriptor",
            code: &[0x66, 0x0F, 0x38, 0x82, 0x03, 0xF4],
            vector: 13,
            invpcid_type: 0,
            descriptor_low: 1 << 12,
            descriptor_linear: TEST_VADDR,
        },
        Case {
            name: "noncanonical-linear",
            code: &[0x66, 0x0F, 0x38, 0x82, 0x03, 0xF4],
            vector: 13,
            invpcid_type: 0,
            descriptor_low: 0,
            descriptor_linear: 0x0000_8000_0000_0000,
        },
        Case {
            name: "noncanonical-source-gp",
            code: &[0x66, 0x0F, 0x38, 0x82, 0x03, 0xF4],
            vector: 13,
            invpcid_type: 2,
            descriptor_low: 0,
            descriptor_linear: TEST_VADDR,
        },
        Case {
            name: "noncanonical-source-ss",
            code: &[0x66, 0x0F, 0x38, 0x82, 0x04, 0x24, 0xF4],
            vector: 12,
            invpcid_type: 2,
            descriptor_low: 0,
            descriptor_linear: TEST_VADDR,
        },
    ];

    for case in cases {
        let (mut vcpu, memory) = paged_vcpu(case.code, case.descriptor_low, case.descriptor_linear);
        seed_stale_translation(&mut vcpu, &memory);
        if case.name == "APX" {
            vcpu.regs.r16 = case.invpcid_type;
            vcpu.regs.r17 = DESCRIPTOR_VADDR;
        } else {
            configure_legacy(&mut vcpu, case.invpcid_type);
        }
        if case.name == "CPL" {
            vcpu.sregs.cs.selector = 3;
        }
        if case.name == "noncanonical-source-gp" {
            vcpu.regs.rbx = 0x0000_8000_0000_0000;
        }
        if case.name == "cross-page-fault" {
            vcpu.regs.rbx = DESCRIPTOR_VADDR + 0xFF8;
        }
        if case.name == "noncanonical-source-ss" {
            vcpu.regs.rsp = 0x0000_8000_0000_0000;
        }
        if case.name == "page-fault" {
            memory
                .write_slice(
                    &0_u64.to_le_bytes(),
                    GuestAddress(PT + (DESCRIPTOR_VADDR >> 12) * 8),
                )
                .unwrap();
        }
        let initial_flags = vcpu.regs.rflags;
        let region = Arc::new(
            vcpu.jit_compile_region()
                .expect("compile dynamically guarded INVPCID")
                .expect("dynamic INVPCID failure must not block native admission"),
        );
        let cache_key = (vcpu.regs.rip, vcpu.jit_mode_tag());
        vcpu.jit_cache.insert(cache_key, Some(region.clone()));
        vcpu.jit_hot.insert(0x2000, 7);
        vcpu.jit_ineligible.insert((0x3000, 0), vec![0xF4]);
        vcpu.jit_ineligible_dirty.insert((0x3000, 0));

        vcpu.jit_run_region_native(&region);

        assert_eq!(vcpu.regs.rip, 0, "{}: exact fault replay PC", case.name);
        assert_eq!(vcpu.regs.rflags, initial_flags, "{}", case.name);
        assert_eq!(observed_translation(&mut vcpu), OLD_PAGE, "{}", case.name);
        assert!(vcpu.jit_cache.contains_key(&cache_key), "{}", case.name);
        assert_eq!(vcpu.jit_hot.get(&0x2000), Some(&7), "{}", case.name);
        assert!(
            vcpu.jit_ineligible.contains_key(&(0x3000, 0)),
            "{}",
            case.name
        );
        assert!(
            vcpu.jit_ineligible_dirty.contains(&(0x3000, 0)),
            "{}",
            case.name
        );

        let error = format!("{:#}", vcpu.step().expect_err("direct replay must fault"));
        let exact_fault = if case.vector == 14 {
            error.contains("Page fault at vaddr") || error.contains("IDT entry 14 not present")
        } else {
            error.contains(&format!("IDT entry {} not present", case.vector))
        };
        assert!(
            exact_fault,
            "{}: wrong exception priority: {error}",
            case.name
        );
        assert_eq!(vcpu.regs.rip, 0, "{}", case.name);
    }
}
