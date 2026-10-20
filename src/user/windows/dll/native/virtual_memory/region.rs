//! Allocation-wide information with build29683 variable-prefix copy semantics.
use super::{compatibility_length, process_status};
use crate::user::windows::arch::WinArch;
use crate::user::windows::dll::native::query::probe_write;
use crate::user::windows::hle::{ApiResult, Ctx, Flow};
use crate::user::windows::memory::{Mem, MemFault};
use crate::user::windows::nt::status::{
    STATUS_DATATYPE_MISALIGNMENT, STATUS_INFO_LENGTH_MISMATCH, STATUS_INVALID_ADDRESS,
    STATUS_INVALID_PARAMETER, STATUS_SUCCESS,
};

pub(super) fn query(
    c: &mut Ctx,
    process: u64,
    address: u64,
    class: u32,
    output: u64,
    length: u64,
    returned: u64,
) -> ApiResult {
    let wow = c.arch() == WinArch::X86;
    let minimum = match (wow, class) {
        (true, 3) => 16,
        (true, 7) => 20,
        (false, 3) => 24,
        (false, 7) => 32,
        _ => unreachable!("region query class"),
    };
    if length < minimum {
        return Flow::ret(STATUS_INFO_LENGTH_MISMATCH.into());
    }
    if address >= c.p.vm.high() {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    if !wow {
        if output % 8 != 0 {
            return Flow::ret(STATUS_DATATYPE_MISALIGNMENT.into());
        }
        let size = usize::try_from(length).map_err(|_| MemFault {
            addr: output,
            write: true,
        })?;
        probe_write(c, output, size)?;
        if returned != 0 {
            probe_write(c, returned, c.psize() as usize)?;
        }
    }
    let status = process_status(c, process)?;
    if status != STATUS_SUCCESS {
        if wow {
            compatibility_length(c, returned, Some(0));
        }
        return Flow::ret(status.into());
    }
    let Some(region) = c.p.vm.allocation_region(address) else {
        if wow {
            compatibility_length(c, returned, Some(0));
        }
        return Flow::ret(STATUS_INVALID_ADDRESS.into());
    };
    let width = c.psize() as usize;
    let size_offset = width + 8;
    let mut bytes = [0u8; 48];
    bytes[..width].copy_from_slice(&region.base.to_le_bytes()[..width]);
    bytes[width..width + 4].copy_from_slice(&region.protect.to_le_bytes());
    let kind = if class == 3 {
        region.kind
    } else {
        region.flags
    };
    bytes[width + 4..width + 8].copy_from_slice(&kind.to_le_bytes());
    for (index, value) in [region.size, region.commit, 0, region.node]
        .into_iter()
        .enumerate()
    {
        let offset = size_offset + index * width;
        bytes[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
    }
    // Copy complete pointer-sized fields only; oversized native buffers were
    // already probed in full, including the tail that is left untouched.
    let copied = (length / width as u64 * width as u64).min((size_offset + 4 * width) as u64);
    c.mem().wr(output, &bytes[..copied as usize])?;
    if wow {
        compatibility_length(c, returned, Some(copied as u32));
    } else if returned != 0 {
        c.write_ptr(returned, copied)?;
    }
    Flow::ret(STATUS_SUCCESS.into())
}
