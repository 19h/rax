//! Process-local wait packets; installed-build construction and wait semantics.
//! Association/cancellation follow installed-build lifecycle and fault captures.
use super::object_create::{self, Attributes};
use super::query::{guard_result, probe_write};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiResult, Ctx, Flow};
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::{CompletionPacket, ObjId, Object};
use crate::user::windows::{memory::Mem, sync};

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

fn packet_handle(c: &Ctx, handle: u64, port: bool) -> Result<ObjId, u32> {
    if handle == c.arch().ptr(u64::MAX) || handle == c.arch().ptr(u64::MAX - 1) {
        return Err(STATUS_OBJECT_TYPE_MISMATCH);
    }
    let id = c.p.objects.id(handle).ok_or(STATUS_INVALID_HANDLE)?;
    let correct = if port {
        matches!(c.p.objects.obj(id), Some(Object::CompletionPort { .. }))
    } else {
        matches!(
            c.p.objects.obj(id),
            Some(Object::WaitCompletionPacket { .. })
        )
    };
    if !correct {
        return Err(STATUS_OBJECT_TYPE_MISMATCH);
    }
    let right = if port { 2 } else { 1 };
    if c.p.objects.access(handle).unwrap_or(0) & right == 0 {
        return Err(STATUS_ACCESS_DENIED);
    }
    Ok(id)
}

pub(super) fn associate(c: &mut Ctx) -> ApiResult {
    // Contexts and information are opaque values. Only the optional BOOLEAN
    // destination is written, and only after successful publication.
    let (packet, port, target, key, apc, status, information, already) = (
        c.ptr(0)?,
        c.ptr(1)?,
        c.ptr(2)?,
        c.ptr(3)?,
        c.ptr(4)?,
        c.u32(5)?,
        c.ptr(6)?,
        c.ptr(7)?,
    );
    let packet = match packet_handle(c, packet, false) {
        Ok(id) => id,
        Err(status) => return Flow::ret(status.into()),
    };
    let port = match packet_handle(c, port, true) {
        Ok(id) => id,
        Err(status) => return Flow::ret(status.into()),
    };
    let target = if target == c.arch().ptr(u64::MAX - 1) {
        c.t.obj
    } else if target == c.arch().ptr(u64::MAX) {
        let Some(id) = c.p.objects.iter().find_map(|(id, object)| {
            matches!(object, Object::Process { pid, .. } if *pid == c.p.pid).then_some(id)
        }) else {
            return Flow::ret(STATUS_INVALID_HANDLE.into());
        };
        id
    } else {
        let Some(id) = c.p.objects.id(target) else {
            return Flow::ret(STATUS_INVALID_HANDLE.into());
        };
        // No expected object type is requested for this handle: SYNCHRONIZE
        // precedes dispatcher eligibility (unlike the two typed handles).
        if c.p.objects.access(target).unwrap_or(0) & 0x0010_0000 == 0 {
            return Flow::ret(STATUS_ACCESS_DENIED.into());
        }
        id
    };
    match c.p.objects.obj(target) {
        Some(
            Object::Event { .. }
            | Object::Timer { .. }
            | Object::IrTimer { .. }
            | Object::Semaphore { .. }
            | Object::Thread { .. }
            | Object::Process { .. }
            | Object::File(_)
            | Object::Console(_)
            | Object::Null
            | Object::Pipe { .. },
        ) => {}
        Some(
            Object::Mutex { .. }
            | Object::WaitCompletionPacket { .. }
            | Object::CompletionPort { .. },
        ) => {
            return Flow::ret(STATUS_INVALID_PARAMETER_3.into());
        }
        Some(_) => return Err(c.unsupported("wait completion packet target dispatcher type")),
        None => return Flow::ret(STATUS_INVALID_HANDLE.into()),
    }
    sync::pump_wait_packets(c.p)?;
    if c.p.objects.wait_packet_busy(packet) {
        return Flow::ret(STATUS_INVALID_PARAMETER_1.into());
    }
    c.p.objects
        .associate_wait_packet(
            packet,
            port,
            target,
            CompletionPacket::new(key, apc, status, information),
        )
        .map_err(|message| crate::user::windows::hle::ApiErr::Internal(message.into()))?;
    sync::pump_wait_packets(c.p)?;
    let signaled = c.p.objects.wait_packet_target(packet).is_none();
    if already != 0
        && let Err(fault) = c.mem().wr(already, &[u8::from(signaled)])
    {
        // Measured native/WoW64 entries suppress this store's exception without
        // rolling back the association. A guard is consumed once, without retry.
        c.p.vm.take_guard(fault.addr);
    }
    Flow::ret(0)
}

pub(super) fn cancel(c: &mut Ctx) -> ApiResult {
    let (handle, remove) = (c.ptr(0)?, c.u32(1)? as u8 != 0);
    let packet = match packet_handle(c, handle, false) {
        Ok(id) => id,
        Err(status) => return Flow::ret(status.into()),
    };
    sync::pump_wait_packets(c.p)?;
    Flow::ret(c.p.objects.cancel_wait_packet(packet, remove).into())
}
