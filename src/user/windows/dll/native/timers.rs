//! Installed-build29683 extended timer construction; arming has its own service.
use super::object_create;
use super::query::{guard_result, probe_write};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiErr, ApiResult, Ctx, Flow, Value};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;

mod arming;
pub(super) use arming::{cancel, set};

const IR: u32 = 2;
const NO_WAKE: u32 = 8;
const VALID: u32 = 0x8000_000E;

fn nt_result(c: &mut Ctx, result: ApiResult) -> ApiResult {
    match guard_result(c, result) {
        Err(ApiErr::Fault(_)) => Flow::ret(STATUS_ACCESS_VIOLATION.into()),
        result => result,
    }
}

pub(super) fn create(c: &mut Ctx) -> ApiResult {
    let out = c.ptr(0)?;
    let mut publishing = false;
    let result = checked(c, out, &mut publishing);
    // A fault in the single handle-copy phase rolls back publication. WoW64
    // does not retry that store as a second zero copy after consuming a guard.
    let destination_fault = publishing && matches!(&result, Err(ApiErr::Fault(_)));
    let result = nt_result(c, result);
    if c.arch() == WinArch::X86
        && out != 0
        && !destination_fault
        && matches!(&result, Ok(Flow::Ret(Value::Int(status))) if (*status as u32 as i32) < 0)
    {
        if let Err(fault) = c.write_ptr(out, 0) {
            return nt_result(c, Err(fault.into()));
        }
    }
    result
}

fn checked(c: &mut Ctx, out: u64, publishing: &mut bool) -> ApiResult {
    let id_address = c.ptr(1)?;
    let object_attributes = c.ptr(2)?;
    let attributes = c.u32(3)?;
    let access = c.u32(4)?;
    // These checks precede every guest pointer probe. ObjectAttributes is
    // rejected by value, including an inaccessible or guard-backed address.
    if attributes & !VALID != 0 || attributes & (IR | NO_WAKE) == (IR | NO_WAKE) {
        return Flow::ret(STATUS_INVALID_PARAMETER_4.into());
    }
    if object_attributes != 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER_3.into());
    }
    if attributes & IR == 0 && id_address != 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER_2.into());
    }
    let wow = c.arch() == WinArch::X86;
    // Native64 probes PHANDLE before the optional ULONG read. WoW64 captures
    // a non-null IR ID first and later copies its temporary HANDLE. A null
    // output is passed through and fails before ID capture on both paths.
    if !wow || out == 0 {
        probe_write(c, out, c.psize() as usize)?;
    }
    let timer_id = if attributes & IR != 0 {
        let value = if id_address == 0 {
            0
        } else {
            c.mem().u32(id_address)?
        };
        // Measured UserMode role outcomes survive disabling all privileges.
        // Preserve the selected build's profile; no host timer ID is acquired.
        match value {
            0 | 20..=u32::MAX => return Flow::ret(STATUS_INVALID_PARAMETER.into()),
            2..=4 | 6..=7 | 17 => return Flow::ret(STATUS_ACCESS_DENIED.into()),
            _ => Some(value),
        }
    } else {
        None
    };
    let grant = object_create::desired_access(c, access, "extended timer")?;
    *publishing = true;
    if wow {
        probe_write(c, out, 4)?;
    }
    object_create::publish(
        c,
        out,
        false,
        grant,
        Object::IrTimer {
            attributes,
            timer_id,
            signaled: false,
            schedule: None,
        },
    )
}
