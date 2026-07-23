//! Native x86 guest-region metadata shared by compilation and execution.

use super::X86_64Vcpu;
use crate::smir::ir::SmirFunction;

/// A compiled native hot-block region. The lowered code is register-state
/// independent (it marshals guest state in/out per run), so one `JitRegion` is
/// cached by (RIP, mode_tag) and re-run for every later entry to that RIP until
/// an underlying guest source page is written (SMC invalidation).
pub(super) struct JitRegion {
    pub(super) exec: crate::smir::lower::runtime::ExecMem,
    pub(super) entry_offset: usize,
    /// Sorted, deduplicated 4 KiB virtual pages containing every instruction
    /// byte executed by this region. Compilation reads at most 512 contiguous
    /// bytes, so this is currently one or two pages. Keeping the complete set
    /// makes execution-time SMC protection independent of prior interpreter
    /// fetches and keeps cache invalidation exact if the lift window changes.
    pub(super) source_pages: Vec<u64>,
    /// Whether the entry trampoline must marshal ZMM0-ZMM31 and K0-K7.
    #[cfg(target_arch = "x86_64")]
    pub(super) uses_vector: bool,
    /// Whether state-backed XMM operations (including helper-backed masked and
    /// SSE4A scalar stores) need vector state copied through `GuestRegs`
    /// without activating the native vector entry bridge.
    #[cfg(target_arch = "x86_64")]
    pub(super) uses_xmm_state: bool,
    /// Whether vector state can use AVX512F KMOVW while retaining K[63:16] in
    /// memory. False selects the general AVX512BW KMOVQ path.
    #[cfg(target_arch = "x86_64")]
    pub(super) narrow_vector_opmasks: bool,
    /// Whether the native entry bridge must marshal MM0-MM7 and enter MMX state.
    #[cfg(target_arch = "x86_64")]
    pub(super) uses_mmx: bool,
    /// Whether an x87/MMX state marker reads or commits the guest tag word.
    /// `EMMS` needs this channel without activating MM0-MM7 marshalling.
    #[cfg(target_arch = "x86_64")]
    pub(super) uses_x87_tag_state: bool,
    /// Whether the region reads the real-time guest timestamp counter. Such a
    /// region cannot be replayed bit-for-bit by RAX_JIT_VERIFY because its
    /// interpreter replay necessarily observes a later clock value.
    #[cfg(target_arch = "x86_64")]
    pub(super) uses_timestamp: bool,
    /// Guest PCs used as resume targets by synthesized backward-edge exits.
    /// Verification must observe the actual backward transition to one of
    /// these PCs, rather than stopping at an earlier forward arrival at the
    /// same internal block.
    #[cfg(target_arch = "x86_64")]
    pub(super) yielded_backward_exit_pcs: Vec<u64>,
    /// `(call_pc, return_pc)` pairs for near CALLs lowered through the direct
    /// interpreter helper. Verification must ignore any coincidental visit to
    /// the region's final exit PC while such a callee is still active, matching
    /// the helper's own run-until-return contract.
    #[cfg(target_arch = "x86_64")]
    pub(super) callout_boundaries: Vec<(u64, u64)>,
}

impl JitRegion {
    /// Derive the exact source-page set from pre-optimization instruction
    /// provenance. Returning `None` fails closed for an empty region or an
    /// instruction whose inclusive end overflows the linear-address space. For
    /// I retained instructions, sorting takes O(I log I) time and O(I) space.
    pub(super) fn collect_source_pages(func: &SmirFunction) -> Option<Vec<u64>> {
        let mut pages = Vec::new();
        for (&(_, pc), instruction) in &func.x86_instruction_bytes {
            let span = instruction.as_slice().len().checked_sub(1)? as u64;
            let end = pc.checked_add(span)?;
            pages.push(pc & !0xFFF);
            pages.push(end & !0xFFF);
        }
        pages.sort_unstable();
        pages.dedup();
        (!pages.is_empty()).then_some(pages)
    }
}

impl X86_64Vcpu {
    /// Establish the active native region before any helper can access guest
    /// memory. Marking is idempotent and costs O(P), where P is the number of
    /// source pages (currently P <= 2 for the 512-byte lift window).
    pub(super) fn jit_enter_region(&mut self, region: &JitRegion) {
        self.jit_active_source_range = region
            .source_pages
            .first()
            .copied()
            .zip(region.source_pages.last().copied());
        self.jit_active_region_stale = false;
        for &page in &region.source_pages {
            self.mmu.mark_code_page(page);
        }
    }

    /// Clear transient active-region state after the native trampoline has
    /// returned and all architectural state has been imported.
    pub(super) fn jit_leave_region(&mut self) {
        self.jit_active_source_range = None;
        self.jit_active_region_stale = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::ir::X86InstructionBytes;
    use crate::smir::ir::types::{BlockId, FunctionId};
    use std::sync::Arc;
    use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

    fn test_vcpu() -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        let mut vcpu = X86_64Vcpu::new(0, memory.clone());
        vcpu.sregs.cr0 = 0x21;
        vcpu.sregs.efer = 1 << 10;
        vcpu.sregs.cs.l = true;
        vcpu.regs.rflags = 0x2;
        (vcpu, memory)
    }

    #[test]
    fn source_page_collection_fails_closed_on_missing_or_wrapping_provenance() {
        let mut function = SmirFunction::new(FunctionId(0), BlockId(0), 0);
        assert_eq!(JitRegion::collect_source_pages(&function), None);

        function.x86_instruction_bytes.insert(
            (BlockId(0), u64::MAX),
            X86InstructionBytes::new(&[0x66, 0x90]).unwrap(),
        );
        assert_eq!(JitRegion::collect_source_pages(&function), None);
    }

    #[test]
    fn region_entry_marks_cross_page_sources_on_every_run_but_compilation_does_not() {
        let (mut vcpu, memory) = test_vcpu();
        let entry = 0x2FFD;
        // MOV EAX,1 crosses from page 0x2000 into page 0x3000; RET is an
        // interpreter frontier and is deliberately absent from native source
        // provenance.
        memory
            .write_slice(&[0xB8, 1, 0, 0, 0, 0xC3], GuestAddress(entry))
            .unwrap();
        vcpu.regs.rip = entry;

        assert!(!vcpu.mmu.is_code_page(0x2000));
        assert!(!vcpu.mmu.is_code_page(0x3000));
        let region = vcpu
            .jit_compile_region()
            .unwrap()
            .expect("cross-page MOV should compile");
        assert_eq!(region.source_pages, vec![0x2000, 0x3000]);
        assert!(
            !vcpu.mmu.is_code_page(0x2000) && !vcpu.mmu.is_code_page(0x3000),
            "lookahead and successful compilation are not execution"
        );

        for run in 0..2 {
            vcpu.jit_enter_region(&region);
            assert!(vcpu.mmu.is_code_page(0x2000));
            assert!(vcpu.mmu.is_code_page(0x3000));
            assert_eq!(vcpu.jit_active_source_range, Some((0x2000, 0x3000)));
            if run == 0 {
                vcpu.invalidate_code_page(0x4000);
                assert!(!vcpu.jit_active_region_stale);
                vcpu.invalidate_code_page(0x3000);
                assert!(vcpu.jit_active_region_stale);
            } else {
                assert!(
                    !vcpu.jit_active_region_stale,
                    "each entry resets transient stale state"
                );
            }
            vcpu.jit_leave_region();
            assert_eq!(vcpu.jit_active_source_range, None);
            vcpu.mmu.clear_code_pages();
        }

        // Cache invalidation follows retained provenance rather than inferring
        // overlap solely from the cache key. The deliberately unrelated key
        // makes this assertion distinguish the two mechanisms.
        let unrelated_key = (0x7000, vcpu.jit_mode_tag());
        vcpu.jit_cache.insert(unrelated_key, Some(Arc::new(region)));
        vcpu.invalidate_code_page(0x3000);
        assert!(!vcpu.jit_cache.contains_key(&unrelated_key));

        let frontier = 0x5000;
        memory.write_slice(&[0xC3], GuestAddress(frontier)).unwrap();
        vcpu.regs.rip = frontier;
        assert!(vcpu.jit_compile_region().unwrap().is_none());
        assert!(
            !vcpu.mmu.is_code_page(frontier),
            "ineligible lookahead must not classify bytes as executed code"
        );
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn scalar_store_helper_deopts_when_only_the_last_byte_is_on_a_code_page() {
        use super::super::rax_jit_mem_store;

        let (mut vcpu, memory) = test_vcpu();
        let addr = 0x2FFC;
        let before = [0xCC_u8; 8];
        memory.write_slice(&before, GuestAddress(addr)).unwrap();
        vcpu.mmu.mark_code_page(0x3000);
        assert!(!vcpu.mmu.is_code_page(addr));

        assert_eq!(
            unsafe {
                rax_jit_mem_store(
                    &mut vcpu,
                    addr,
                    0x8877_6655_4433_2211,
                    std::mem::size_of::<u64>() as u32,
                )
            },
            0
        );
        let mut after = [0_u8; 8];
        memory.read_slice(&mut after, GuestAddress(addr)).unwrap();
        assert_eq!(after, before);
    }
}
