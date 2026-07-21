//! Direct/native INVLPG differentials over stale translation state.

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
const TEST_VADDR: u64 = 0x7000;
const PAGE_FLAGS: u64 = 0x7; // Present | writable | user-accessible.

fn paged_vcpu(code: &[u8]) -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    for (address, entry) in [
        (PML4, PDPT | PAGE_FLAGS),
        (PDPT, PD | PAGE_FLAGS),
        (PD, PT | PAGE_FLAGS),
        (PT, CODE_PAGE | PAGE_FLAGS),
        (PT + (TEST_VADDR >> 12) * 8, OLD_PAGE | PAGE_FLAGS),
    ] {
        memory
            .write_slice(&entry.to_le_bytes(), GuestAddress(address))
            .unwrap();
    }
    memory.write_slice(code, GuestAddress(CODE_PAGE)).unwrap();

    let mut vcpu = X86_64Vcpu::new(0, memory.clone());
    vcpu.sregs.efer = (1 << 8) | (1 << 10);
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.sregs.cr0 = 0x8005_0033;
    vcpu.sregs.cr3 = PML4;
    vcpu.sregs.cr4 = 1 << 5;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x9000;
    // linux/amd64 user-mode emulation on an Arm host clears an imported AF
    // across pushfq/popfq. Keep every other modeled status/control bit set;
    // the emitted pushfq/popfq restoration sequence is asserted independently.
    vcpu.regs.rflags = (0x2 | 0x08D5 | flags::bits::DF) & !flags::bits::AF;
    vcpu.set_jit_mem(false);
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

#[test]
fn direct_invlpg_flushes_a_canonical_translation_but_noncanonical_is_a_true_nop() {
    for (name, addr, expected_page, expect_decode_flush) in [
        ("canonical", TEST_VADDR, NEW_PAGE, true),
        (
            "noncanonical",
            0x0000_8000_0000_0000 | TEST_VADDR,
            OLD_PAGE,
            false,
        ),
    ] {
        let (mut vcpu, memory) = paged_vcpu(&[0x0F, 0x01, 0x38, 0xF4]);
        seed_stale_translation(&mut vcpu, &memory);
        vcpu.regs.rax = addr;
        let initial_flags = vcpu.regs.rflags;

        assert!(vcpu.step().expect("direct INVLPG").is_none(), "{name}");

        assert_eq!(vcpu.regs.rip, 3, "{name}");
        assert_eq!(vcpu.regs.rflags, initial_flags, "{name}");
        assert_eq!(observed_translation(&mut vcpu), expected_page, "{name}");
        assert_eq!(
            vcpu.decode_cache[X86_64Vcpu::decode_cache_index(0)].bytes_len == 0,
            expect_decode_flush,
            "{name}: decode-cache invalidation"
        );
    }
}

#[test]
fn native_invlpg_matches_direct_translation_and_cache_invalidation() {
    for (name, code, addr, apx_enabled, expected_page, expect_cache_flush) in [
        (
            "canonical",
            &[0x0F, 0x01, 0x38, 0xEB, 0x00, 0xF4][..],
            TEST_VADDR,
            false,
            NEW_PAGE,
            true,
        ),
        (
            "noncanonical",
            &[0x0F, 0x01, 0x38, 0xEB, 0x00, 0xF4],
            0x0000_8000_0000_0000 | TEST_VADDR,
            false,
            OLD_PAGE,
            false,
        ),
        (
            "rex2-canonical",
            &[0xD5, 0x80, 0x01, 0x38, 0xEB, 0x00, 0xF4],
            TEST_VADDR,
            true,
            NEW_PAGE,
            true,
        ),
    ] {
        let (mut vcpu, memory) = paged_vcpu(code);
        seed_stale_translation(&mut vcpu, &memory);
        vcpu.regs.rax = addr;
        vcpu.set_apx_enabled(apx_enabled);
        let initial_flags = vcpu.regs.rflags;
        let region = Arc::new(
            vcpu.jit_compile_region()
                .expect("compile INVLPG region")
                .expect("INVLPG must remain native eligible without memory helpers"),
        );
        let cache_key = (vcpu.regs.rip, vcpu.jit_mode_tag());
        vcpu.jit_cache.insert(cache_key, Some(region.clone()));
        vcpu.jit_hot.insert(0x2000, 7);
        vcpu.jit_ineligible.insert((0x3000, 0), vec![0xF4]);
        vcpu.jit_ineligible_dirty.insert((0x3000, 0));

        vcpu.jit_run_region_native(&region);

        assert_eq!(
            vcpu.regs.rip,
            if apx_enabled { 4 } else { 3 },
            "{name}: exact post-instruction exit"
        );
        assert_eq!(vcpu.regs.rflags, initial_flags, "{name}");
        assert_eq!(observed_translation(&mut vcpu), expected_page, "{name}");
        assert_eq!(vcpu.jit_cache.is_empty(), expect_cache_flush, "{name}");
        assert_eq!(vcpu.jit_hot.is_empty(), expect_cache_flush, "{name}");
        assert_eq!(vcpu.jit_ineligible.is_empty(), expect_cache_flush, "{name}");
        assert_eq!(
            vcpu.jit_ineligible_dirty.is_empty(),
            expect_cache_flush,
            "{name}"
        );
    }
}

#[test]
fn verified_invlpg_replays_to_the_exact_frontier_after_cache_invalidation() {
    let (mut vcpu, memory) = paged_vcpu(&[0x0F, 0x01, 0x38, 0xF4]);
    seed_stale_translation(&mut vcpu, &memory);
    vcpu.regs.rax = TEST_VADDR;
    let initial_flags = vcpu.regs.rflags;
    let region = Arc::new(
        vcpu.jit_compile_region()
            .expect("compile verified INVLPG region")
            .expect("INVLPG must remain native eligible without memory helpers"),
    );
    let cache_key = (vcpu.regs.rip, vcpu.jit_mode_tag());
    vcpu.jit_cache.insert(cache_key, Some(region.clone()));

    vcpu.jit_run_region_verified(&region);

    assert_eq!(vcpu.regs.rip, 3, "exact post-INVLPG verification frontier");
    assert_eq!(vcpu.regs.rflags, initial_flags);
    assert_eq!(observed_translation(&mut vcpu), NEW_PAGE);
    assert!(
        vcpu.jit_cache.is_empty(),
        "both native execution and interpreter replay must invalidate cached code"
    );
}

#[test]
fn native_invlpg_dynamic_faults_deoptimize_before_any_invalidation() {
    let cases: [(&str, &[u8], fn(&mut X86_64Vcpu), u8); 2] = [
        (
            "protected-cpl3",
            &[0x0F, 0x01, 0x38, 0xF4],
            |vcpu| vcpu.sregs.cs.selector = 3,
            13,
        ),
        (
            "rex2-apx-disabled",
            &[0xD5, 0x80, 0x01, 0x38, 0xF4],
            |_| {},
            6,
        ),
    ];

    for (name, code, configure, expected_vector) in cases {
        let (mut vcpu, memory) = paged_vcpu(code);
        seed_stale_translation(&mut vcpu, &memory);
        vcpu.regs.rax = TEST_VADDR;
        configure(&mut vcpu);
        let initial_flags = vcpu.regs.rflags;
        let region = Arc::new(
            vcpu.jit_compile_region()
                .expect("compile dynamically guarded INVLPG")
                .expect("dynamic INVLPG failure must not block native admission"),
        );
        let cache_key = (vcpu.regs.rip, vcpu.jit_mode_tag());
        vcpu.jit_cache.insert(cache_key, Some(region.clone()));
        vcpu.jit_hot.insert(0x2000, 7);
        vcpu.jit_ineligible.insert((0x3000, 0), vec![0xF4]);
        vcpu.jit_ineligible_dirty.insert((0x3000, 0));

        vcpu.jit_run_region_native(&region);

        assert_eq!(vcpu.regs.rip, 0, "{name}: exact fault replay PC");
        assert_eq!(vcpu.regs.rflags, initial_flags, "{name}");
        assert_eq!(observed_translation(&mut vcpu), OLD_PAGE, "{name}");
        assert!(vcpu.jit_cache.contains_key(&cache_key), "{name}");
        assert_eq!(vcpu.jit_hot.get(&0x2000), Some(&7), "{name}");
        assert!(vcpu.jit_ineligible.contains_key(&(0x3000, 0)), "{name}");
        assert!(vcpu.jit_ineligible_dirty.contains(&(0x3000, 0)), "{name}");

        let error = format!("{:#}", vcpu.step().expect_err("direct replay must fault"));
        assert!(
            error.contains(&format!("IDT entry {expected_vector} not present")),
            "{name}: wrong exception priority: {error}"
        );
        assert_eq!(vcpu.regs.rip, 0, "{name}: direct fault PC");
    }
}
