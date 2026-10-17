//! Native class53 pointer admission and process-owned range registration.

use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiResult, Ctx, Flow};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::{
    STATUS_ACCESS_VIOLATION, STATUS_DATATYPE_MISALIGNMENT, STATUS_INFO_LENGTH_MISMATCH,
    STATUS_INVALID_PARAMETER, STATUS_SUCCESS,
};

const INFORMATION_BYTES: usize = 16;
const TABLE_ADDRESS_SPAN: u64 = 56;
const MAXIMUM_ADDRESS_OFFSET: usize = 40;
const BASE_ADDRESS_OFFSET: usize = 48;

pub(super) fn information(c: &mut Ctx, process: u64, input: u64, length: u32) -> ApiResult {
    // The captured WoW64 wrapper returns success before inspecting any of these
    // inputs. This includes null pointers, invalid handles and arbitrary lengths.
    if c.arch() == WinArch::X86 {
        return Flow::ret(STATUS_SUCCESS.into());
    }
    if process != u64::MAX {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    if length != INFORMATION_BYTES as u32 {
        return Flow::ret(STATUS_INFO_LENGTH_MISMATCH.into());
    }
    if input % 8 != 0 {
        return Flow::ret(STATUS_DATATYPE_MISALIGNMENT.into());
    }
    let mut information = [0u8; INFORMATION_BYTES];
    c.mem().rd(input, &mut information)?;
    let mut pointer = [0u8; 8];
    pointer.copy_from_slice(&information[..8]);
    let table = u64::from_le_bytes(pointer);
    if table % 4 != 0 {
        return Flow::ret(STATUS_DATATYPE_MISALIGNMENT.into());
    }
    if table.checked_add(TABLE_ADDRESS_SPAN).is_none() {
        return Flow::ret(STATUS_ACCESS_VIOLATION.into());
    }
    // Native controls leave an inaccessible unused prefix untouched. The base
    // read precedes the maximum read, including when each is on a guard page.
    let base = c.mem().u64(table + BASE_ADDRESS_OFFSET as u64)?;
    let maximum = c.mem().u64(table + MAXIMUM_ADDRESS_OFFSET as u64)?;
    let tables = &mut c.p.seh.kernel_dynamic_tables;
    let status = if information[8] == 0 {
        tables.register(base, maximum)
    } else {
        tables.remove(base, maximum)
    };
    Flow::ret(status.into())
}
