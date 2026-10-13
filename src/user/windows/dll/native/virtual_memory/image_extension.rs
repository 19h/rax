//! Class14 for complete guest images without kernel CFG/SCP extension owners.
//! Native64 absence is a measured successful zero RVA/size. The installed WoW64
//! wrapper rejects this class after its optional ReturnLength preprobe.
use super::{probe_write, process_status};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiResult, Ctx, Flow};
use crate::user::windows::memory::{AllocKind, Mem, MemFault};
use crate::user::windows::nt::status::*;

pub(super) fn query(
    c: &mut Ctx,
    process: u64,
    address: u64,
    output: u64,
    length: u64,
    returned: u64,
) -> ApiResult {
    if c.arch() == WinArch::X86 {
        return Flow::ret(STATUS_INVALID_INFO_CLASS.into());
    }
    if address == 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    if length < 24 {
        return Flow::ret(STATUS_INFO_LENGTH_MISMATCH.into());
    }
    if address >= c.p.vm.high() {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    if output % 8 != 0 {
        return Flow::ret(STATUS_DATATYPE_MISALIGNMENT.into());
    }
    // Native ProbeForWrite rejects an overflowing/out-of-user-range extent
    // before visiting pages. Such rejection must not consume either guard.
    if output >= c.p.vm.high() || length > c.p.vm.high() - output {
        return Flow::ret(STATUS_ACCESS_VIOLATION.into());
    }
    // Probe the complete SIZE_T extent without guest-sized allocation. Native
    // output faults precede ReturnLength, handle rights and input flags/types.
    let bytes = usize::try_from(length).map_err(|_| MemFault {
        addr: output,
        write: true,
    })?;
    probe_write(c, output, bytes)?;
    if returned != 0 {
        if returned >= c.p.vm.high() || 8 > c.p.vm.high() - returned {
            return Flow::ret(STATUS_ACCESS_VIOLATION.into());
        }
        probe_write(c, returned, 8)?;
    }
    let status = process_status(c, process)?;
    if status != STATUS_SUCCESS {
        return Flow::ret(status.into());
    }
    if c.mem().u32(output + 4)? != 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    let Some(allocation) = c.p.vm.allocation(address) else {
        return Flow::ret(STATUS_MEMORY_NOT_ALLOCATED.into());
    };
    if allocation.kind != AllocKind::Image {
        return Flow::ret(STATUS_INVALID_ADDRESS.into());
    }
    if c.mem().u32(output)? >= 2 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    // Current image owners advertise ImageExtensionPresent=0 in class6. Keep
    // their actual guest state: neither native host SCP bytes nor host trust
    // flags are inherited. A future extension owner must extend both queries
    // and own the corresponding mapping/relocation/lifetime as one contract.
    c.mem().wr(output + 8, &[0; 16])?;
    if returned != 0 {
        c.mem().w64(returned, 24)?;
    }
    Flow::ret(STATUS_SUCCESS.into())
}
