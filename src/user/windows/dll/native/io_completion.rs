//! Construct process-local NT I/O completion objects; never host kernel ports.
//! Probe order, flags and unsigned concurrency follow the installed build29683
//! captures. Packet operations and runnable-thread accounting are separate APIs.
use super::object_create::{self, Attributes};
use super::query::{guard_result, probe_write};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiResult, Ctx, Flow};
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;

const KIND: &str = "I/O completion";

pub(super) fn create(c: &mut Ctx) -> ApiResult {
    let result = checked(c);
    guard_result(c, result)
}

fn checked(c: &mut Ctx) -> ApiResult {
    let (out, access, address, concurrency) = (c.ptr(0)?, c.u32(1)?, c.ptr(2)?, c.u32(3)?);
    let wow = c.arch() == WinArch::X86;
    if !wow {
        // Native PHANDLE permits byte alignment, but probes its entire width
        // before inspecting OBJECT_ATTRIBUTES. WoW64 converts attributes first.
        probe_write(c, out, c.psize() as usize)?;
    }
    let attrs = match object_create::capture(c, address, KIND)? {
        Ok(attrs) => attrs,
        Err(status) => return Flow::ret(status.into()),
    };
    if wow {
        probe_write(c, out, 4)?;
    }
    let result = construct(c, out, access, concurrency, attrs);
    object_create::wow_failure(c, out, &result)?;
    result
}

fn construct(c: &mut Ctx, out: u64, access: u32, concurrency: u32, attrs: Attributes) -> ApiResult {
    // Unlike an event, a completion port rejects OBJ_PERMANENT (0x10).
    if attrs.flags & !0x1EE2 != 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    object_create::supported_attributes(c, &attrs, KIND)?;
    if attrs.root != 0 {
        return Flow::ret(STATUS_OBJECT_NAME_INVALID.into());
    }
    let grant = object_create::desired_access(c, access, KIND)?;
    object_create::publish(
        c,
        out,
        attrs.flags & 2 != 0,
        grant,
        Object::CompletionPort {
            queue: Default::default(),
            // The modeled process has one processor (PEB and topology queries).
            // Preserve every nonzero ULONG, including values above i32::MAX.
            concurrency: if concurrency == 0 { 1 } else { concurrency },
        },
    )
}
