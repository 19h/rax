//! Process-local wait packets; installed-build construction and wait semantics.
//! Association, cancellation and completion delivery belong to their services.
use super::object_create::{self, Attributes};
use super::query::{guard_result, probe_write};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiResult, Ctx, Flow};
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;

const KIND: &str = "wait completion packet";

pub(super) fn create(c: &mut Ctx) -> ApiResult {
    let result = checked(c);
    guard_result(c, result)
}

fn checked(c: &mut Ctx) -> ApiResult {
    let (out, access, address) = (c.ptr(0)?, c.u32(1)?, c.ptr(2)?);
    let wow = c.arch() == WinArch::X86;
    if !wow {
        probe_write(c, out, c.psize() as usize)?;
    }
    let attrs = match object_create::capture(c, address, KIND)? {
        Ok(attrs) => attrs,
        Err(status) => return Flow::ret(status.into()),
    };
    if wow {
        probe_write(c, out, 4)?;
    }
    let result = construct(c, out, access, attrs);
    object_create::wow_failure(c, out, &result)?;
    result
}

fn construct(c: &mut Ctx, out: u64, access: u32, attrs: Attributes) -> ApiResult {
    // Object-manager flag validation precedes root/name handling; the packet
    // type's OBJ_PERMANENT rejection follows it (root+permanent => name error).
    if attrs.flags & !0x1EF2 != 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    object_create::supported_attributes(c, &attrs, KIND)?;
    if attrs.root != 0 {
        return Flow::ret(STATUS_OBJECT_NAME_INVALID.into());
    }
    if attrs.flags & 0x10 != 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    if access & 0x0100_0000 != 0 {
        return Err(c.unsupported("NT wait completion packet ACCESS_SYSTEM_SECURITY privilege"));
    }
    // Explicit SYNCHRONIZE is accepted, but generic/maximal mappings exclude
    // it. All three generic R/W/X mappings grant READ_CONTROL + packet right 1.
    let mut grant = access & 0x001F_0001;
    for generic in [0x8000_0000, 0x4000_0000, 0x2000_0000] {
        if access & generic != 0 {
            grant |= 0x0002_0001;
        }
    }
    if access & (0x1000_0000 | 0x0200_0000) != 0 {
        grant |= 0x000F_0001;
    }
    object_create::publish(
        c,
        out,
        attrs.flags & 2 != 0,
        grant,
        Object::WaitCompletionPacket { signaled: true },
    )
}
