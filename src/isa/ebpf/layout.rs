//! Versioned executable regions and fixed-frame virtual stack gaps.

use super::{
    EbpfMemFault, MM_BYTECODE_START, MM_HEAP_START, MM_INPUT_START, MM_REGION_SIZE, MM_STACK_START,
    SbpfMemoryMapping, SbpfMemoryRegion,
};
use crate::isa::ebpf::cpu::EbpfConfig;

impl SbpfMemoryMapping {
    /// Construct an SBPF map from an explicit executable rodata address.
    ///
    /// V0-V2 rodata resides in the bytecode segment; V3/V4 rodata resides in
    /// segment zero. Text starts at MM_BYTECODE_START. A legacy read-only
    /// executable image may contain the text, whose bytes must agree exactly.
    /// Stack size counts backing bytes, excluding V0 virtual gaps. Each access
    /// uses the reference start-address gap check and contiguous backing bounds.
    ///
    /// Construction uses O(stack_size + bytecode.len() + rodata.len() +
    /// heap_size + input.len()) space/time. Region lookup is O(region_count)
    /// in the number of mapped regions; gap translation itself is O(1).
    #[allow(clippy::too_many_arguments)]
    pub fn new_solana_with_config(
        config: &EbpfConfig,
        bytecode: &[u8],
        rodata_addr: u64,
        rodata: &[u8],
        stack_size: usize,
        heap_size: usize,
        input: &[u8],
    ) -> Result<Self, EbpfMemFault> {
        let invalid = |addr, size| EbpfMemFault {
            addr,
            size,
            write: false,
        };
        let Some(version) = config.dialect.sbpf_version() else {
            return Err(invalid(MM_BYTECODE_START, bytecode.len()));
        };
        let rodata_segment = if version.enable_lower_rodata_vaddr() {
            0
        } else {
            MM_BYTECODE_START
        };
        let rodata_end = rodata_addr
            .checked_add(rodata.len() as u64)
            .ok_or_else(|| invalid(rodata_addr, rodata.len()))?;
        if rodata_addr < rodata_segment
            || rodata_end > rodata_segment + MM_REGION_SIZE
            || bytecode.len() as u64 > MM_REGION_SIZE
            || stack_size as u64 > MM_REGION_SIZE
            || heap_size as u64 > MM_REGION_SIZE
            || input.len() as u64 > MM_REGION_SIZE
        {
            return Err(invalid(rodata_addr, rodata.len()));
        }

        let mut map = Self::new();
        map.text = Some(SbpfMemoryRegion::new_readonly(
            MM_BYTECODE_START,
            bytecode.to_vec(),
        ));
        let text_end = MM_BYTECODE_START + bytecode.len() as u64;
        let overlaps_text = !bytecode.is_empty()
            && !rodata.is_empty()
            && rodata_addr < text_end
            && rodata_end > MM_BYTECODE_START;
        if overlaps_text {
            if rodata_addr > MM_BYTECODE_START || rodata_end < text_end {
                return Err(invalid(rodata_addr, rodata.len()));
            }
            let offset = (MM_BYTECODE_START - rodata_addr) as usize;
            if &rodata[offset..offset + bytecode.len()] != bytecode {
                return Err(invalid(MM_BYTECODE_START, bytecode.len()));
            }
        } else if !bytecode.is_empty() && !version.enable_lower_rodata_vaddr() {
            map.add_region(MM_BYTECODE_START, bytecode.to_vec(), false);
        }
        if !rodata.is_empty() {
            map.add_region(rodata_addr, rodata.to_vec(), false);
        }

        if version.stack_frame_gaps() && config.enable_stack_frame_gaps {
            let frame_size = config.stack_frame_size;
            if !frame_size.is_power_of_two() || frame_size > MM_REGION_SIZE {
                return Err(invalid(MM_STACK_START, stack_size));
            }
            if stack_size != 0 {
                let last = stack_size as u64 - 1;
                let virtual_last = (last / frame_size) * 2 * frame_size + last % frame_size;
                if virtual_last >= MM_REGION_SIZE {
                    return Err(invalid(MM_STACK_START, stack_size));
                }
            }
            map.stack_gap_size = Some(frame_size);
        }
        map.add_region(MM_STACK_START, vec![0; stack_size], true);
        map.add_region(MM_HEAP_START, vec![0; heap_size], true);
        map.add_region(MM_INPUT_START, input.to_vec(), true);
        Ok(map)
    }
}
