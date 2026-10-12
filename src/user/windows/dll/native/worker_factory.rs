//! Measured installed-build29683 worker-factory construction. Guest requests
//! create local objects; they never create a host worker factory or host thread.
use super::object_create::{self, Attributes};
use super::query::{guard_result, probe_write};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiErr, ApiResult, Ctx, Flow};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::{ObjId, Object};

const KIND: &str = "worker factory";
// The installed contract includes specific bits0x40/0x80 beyond the pinned
// PHNT WORKER_FACTORY_ALL_ACCESS declaration. Native grants own this mapping.
const ALL_ACCESS: u32 = 0x000F_00FF;

struct Request {
    out: u64,
    access: u32,
    attrs: u64,
    port: u64,
    process: u64,
    start: u64,
    parameter: u64,
    maximum: u32,
    reserve: u64,
    commit: u64,
}

pub(super) fn create(c: &mut Ctx) -> ApiResult {
    let result = checked(c);
    guard_result(c, result)
}

fn checked(c: &mut Ctx) -> ApiResult {
    let r = Request {
        out: c.ptr(0)?,
        access: c.u32(1)?,
        attrs: c.ptr(2)?,
        port: c.ptr(3)?,
        process: c.ptr(4)?,
        start: c.ptr(5)?,
        parameter: c.ptr(6)?,
        maximum: c.u32(7)?,
        reserve: c.ptr(8)?,
        commit: c.ptr(9)?,
    };
    let wow = c.arch() == WinArch::X86;
    // WoW64 converts attributes before entering the kernel. Native validation
    // instead probes PHANDLE, checks both handles, then captures attributes.
    let attrs = if wow {
        match object_create::capture(c, r.attrs, KIND)? {
            Ok(attrs) => Some(attrs),
            Err(status) => return Flow::ret(status.into()),
        }
    } else {
        None
    };
    probe_write(c, r.out, c.psize() as usize)?;
    let result = construct(c, &r, attrs);
    object_create::wow_failure(c, r.out, &result)?;
    result
}

fn handles(
    c: &Ctx,
    port: u64,
    process: u64,
) -> Result<Result<(ObjId, Option<ObjId>), u32>, ApiErr> {
    if port == c.arch().ptr(u64::MAX) || port == c.arch().ptr(u64::MAX - 1) {
        return Ok(Err(STATUS_OBJECT_TYPE_MISMATCH));
    }
    let Some(port_id) = c.p.objects.id(port) else {
        return Ok(Err(STATUS_INVALID_HANDLE));
    };
    if !matches!(
        c.p.objects.obj(port_id),
        Some(Object::CompletionPort { .. })
    ) {
        return Ok(Err(STATUS_OBJECT_TYPE_MISMATCH));
    }
    if c.p.objects.access(port).unwrap_or(0) & 2 == 0 {
        return Ok(Err(STATUS_ACCESS_DENIED));
    }
    if process == c.arch().ptr(u64::MAX) {
        return Ok(Ok((port_id, None)));
    }
    if process == c.arch().ptr(u64::MAX - 1) {
        return Ok(Err(STATUS_OBJECT_TYPE_MISMATCH));
    }
    let Some(id) = c.p.objects.id(process) else {
        return Ok(Err(STATUS_INVALID_HANDLE));
    };
    let Some(Object::Process { pid, .. }) = c.p.objects.obj(id) else {
        return Ok(Err(STATUS_OBJECT_TYPE_MISMATCH));
    };
    // PROCESS_CREATE_THREAD | PROCESS_VM_OPERATION | PROCESS_VM_WRITE.
    if c.p.objects.access(process).unwrap_or(0) & 0x2A != 0x2A {
        return Ok(Err(STATUS_ACCESS_DENIED));
    }
    if *pid != c.p.pid {
        return Err(c.unsupported("NT worker factory cross-process target"));
    }
    Ok(Ok((port_id, Some(id))))
}

fn construct(c: &mut Ctx, r: &Request, attrs: Option<Attributes>) -> ApiResult {
    let (port, process) = match handles(c, r.port, r.process)? {
        Ok(ids) => ids,
        Err(status) => return Flow::ret(status.into()),
    };
    let attrs = if let Some(attrs) = attrs {
        attrs
    } else {
        match object_create::capture(c, r.attrs, KIND)? {
            Ok(attrs) => attrs,
            Err(status) => return Flow::ret(status.into()),
        }
    };
    if attrs.flags & !0x1EF2 != 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    object_create::supported_attributes(c, &attrs, KIND)?;
    if attrs.root != 0 {
        return Flow::ret(STATUS_OBJECT_NAME_INVALID.into());
    }
    if r.access & 0x0100_0000 != 0 {
        return Err(c.unsupported("NT worker factory ACCESS_SYSTEM_SECURITY privilege"));
    }
    let mut grant = r.access & ALL_ACCESS;
    for (generic, specific) in [
        (0x8000_0000, 0x0002_0008),
        (0x4000_0000, 0x0002_0004),
        (0x2000_0000, 0x0002_0003),
        (0x1000_0000, ALL_ACCESS),
        (0x0200_0000, ALL_ACCESS),
    ] {
        if r.access & generic != 0 {
            grant |= specific;
        }
    }
    object_create::publish(
        c,
        r.out,
        attrs.flags & 2 != 0,
        grant,
        Object::WorkerFactory {
            port,
            process,
            pid: c.p.pid,
            start: r.start,
            parameter: r.parameter,
            maximum: r.maximum,
            soft_maximum: None,
            binding_count: 0,
            // Native constructor defaults are independent of the executable's PE
            // stack sizes. Nonzero requests are stored without rounding/probing.
            reserve: if r.reserve == 0 { 65536 } else { r.reserve },
            commit: if r.commit == 0 { 4096 } else { r.commit },
        },
    )
}

pub(super) fn set_information(c: &mut Ctx) -> ApiResult {
    let result = set_properties(c);
    guard_result(c, result)
}

fn set_properties(c: &mut Ctx) -> ApiResult {
    let (handle, class, input, length) = (c.ptr(0)?, c.u32(1)?, c.ptr(2)?, c.u32(3)?);
    if !matches!(class, 3 | 5 | 14) {
        return Err(c.unsupported(format!("NtSetInformationWorkerFactory class {class}")));
    }
    // The installed native/WoW64 entries check exact ULONG size and capture
    // the input before handle lookup. Unaligned and readonly inputs are valid.
    if length != 4 {
        return Flow::ret(STATUS_INFO_LENGTH_MISMATCH.into());
    }
    let value = c.mem().u32(input)?;
    if handle == c.arch().ptr(u64::MAX) || handle == c.arch().ptr(u64::MAX - 1) {
        return Flow::ret(STATUS_OBJECT_TYPE_MISMATCH.into());
    }
    let Some(id) = c.p.objects.id(handle) else {
        return Flow::ret(STATUS_INVALID_HANDLE.into());
    };
    if !matches!(c.p.objects.obj(id), Some(Object::WorkerFactory { .. })) {
        return Flow::ret(STATUS_OBJECT_TYPE_MISMATCH.into());
    }
    if c.p.objects.access(handle).unwrap_or(0) & 4 == 0 {
        return Flow::ret(STATUS_ACCESS_DENIED.into());
    }
    let Some(Object::WorkerFactory {
        maximum: hard,
        soft_maximum: soft,
        binding_count: bindings,
        ..
    }) = c.p.objects.obj_mut(id)
    else {
        unreachable!("validated worker factory")
    };
    if class == 3 {
        // The installed kernel treats zero as the all-bits count, while
        // nonzero ULONG bit patterns are signed deltas. Do not wrap either
        // bound, reinterpret this as overwrite, or infer activation policy.
        *bindings = if value == 0 {
            u32::MAX
        } else {
            bindings.saturating_add_signed(value as i32)
        };
    } else if class == 5 {
        *hard = value;
    } else {
        // Native class14 accepts the unsigned request without changing the
        // basic-query hard maximum. Record it independently; activation and
        // the constructor's unqueried soft default are separate contracts.
        *soft = Some(value);
    }
    Flow::ret(0)
}
