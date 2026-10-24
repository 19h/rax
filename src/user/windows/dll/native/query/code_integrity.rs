//! SYSTEM_CODEINTEGRITY_INFORMATION: exact lengths and native copy/alias order.
//! Layout: https://learn.microsoft.com/windows/win32/api/winternl/nf-winternl-ntquerysysteminformation
//! Native four-view controls on build29683 also require Length8 and ULONG
//! alignment on WoW64. The common entry has already probed both destinations.

use super::{ApiResult, Ctx, Flow, Mem, STATUS_INFO_LENGTH_MISMATCH, return_length};

pub(super) fn query(c: &mut Ctx, output: u64, length: u32, returned: u64) -> ApiResult {
    if length != 8 || c.mem().u32(output)? != 8 {
        return_length(c, returned, 8)?;
        return Flow::ret(STATUS_INFO_LENGTH_MISMATCH.into());
    }
    let Some(runtime) = c.p.native.as_ref() else {
        return Err(c.unsupported("SystemCodeIntegrityInformation requires selected native inputs"));
    };
    let options = *runtime.code_integrity.as_ref().map_err(|reason| {
        c.unsupported(format!(
            "SystemCodeIntegrityInformation unavailable: {reason}"
        ))
    })?;
    // The caller's Length survives. Copy options before ReturnLength so an
    // alias of the second DWORD has the measured final value8.
    c.mem().w32(output + 4, options)?;
    return_length(c, returned, 8)?;
    Flow::ret(0)
}
