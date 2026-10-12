//! Measured build29683 NtTraceControl provider/Event registration (15/27).
//! This retains a guest Event; it does not create a host ETW registration.
use super::query::{guard_result, probe_write};
use crate::error::MemoryAccessKind;
use crate::user::windows::hle::{ApiErr, ApiResult, Ctx, Flow};
use crate::user::windows::memory::{Mem, MemFault};
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;

pub(super) fn control(c: &mut Ctx) -> ApiResult {
    let result = checked(c);
    guard_result(c, result)
}
fn checked(c: &mut Ctx) -> ApiResult {
    let function = c.u32(0)?;
    if function != 15 && function != 27 {
        return Err(c.unsupported(format!("NtTraceControl function {function}")));
    }
    let (input, input_bytes, output, output_bytes, returned) =
        (c.ptr(1)?, c.u32(2)?, c.ptr(3)?, c.u32(4)?, c.ptr(5)?);
    // All four measured profiles ignore a NULL optional output, even with a
    // nonzero length. Nonempty non-NULL output is probed before ReturnLength.
    if output != 0 && output_bytes != 0 {
        probe_write(c, output, output_bytes as usize)?;
    }
    if returned == 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    probe_write(c, returned, 4)?;
    if function == 15 {
        return register_provider(c, input, input_bytes, output, output_bytes, returned);
    }
    // The private input is exactly one ULONG in native64 as well as WoW64.
    // Length validation precedes capture; byte-aligned buffers are accepted.
    if input_bytes != 4 || input == 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    let handle = u64::from(c.mem().u32(input)?);
    if handle == 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    let Some(id) = c.p.objects.id(handle) else {
        return Flow::ret(STATUS_INVALID_HANDLE.into());
    };
    if !matches!(c.p.objects.obj(id), Some(Object::Event { .. })) {
        return Flow::ret(STATUS_OBJECT_TYPE_MISMATCH.into());
    }
    if c.p.objects.access(handle).unwrap_or(0) & 2 == 0 {
        return Flow::ret(STATUS_ACCESS_DENIED.into());
    }
    let registered =
        c.p.objects
            .register_etw_event(id)
            .map_err(|message| ApiErr::Internal(message.into()))?;
    if !registered {
        return Flow::ret(STATUS_ALREADY_REGISTERED.into());
    }
    // Capture and publication precede this store, so input/output aliases
    // observe the zero only after the original handle has been retained.
    c.mem().w32(returned, 0)?;
    Flow::ret(STATUS_SUCCESS.into())
}

fn register_provider(
    c: &mut Ctx,
    input: u64,
    input_bytes: u32,
    output: u64,
    output_bytes: u32,
    returned: u64,
) -> ApiResult {
    // Unlike function27, generic function15 capture reaches a nonempty input
    // span before class-specific size checks. NULL input is not read.
    if input != 0 && input_bytes != 0 {
        c.mem()
            .probe(input, input_bytes as usize, MemoryAccessKind::Read)
            .map_err(|fault| MemFault {
                addr: fault.address,
                write: false,
            })?;
    }
    if input == 0 || input_bytes != 160 || output == 0 || output_bytes < 160 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    // Capture before any publication: output/ReturnLength may alias input.
    let bytes = c.mem().bytes(input, 160)?;
    let request: Box<[u8; 160]> = bytes
        .into_boxed_slice()
        .try_into()
        .map_err(|_| ApiErr::Internal("ETW registration capture width".into()))?;
    let mut reply = *request;
    let Some(handle) = c.p.objects.register_etw_provider(request) else {
        return Flow::ret(STATUS_NO_MEMORY.into());
    };
    reply[24..32].copy_from_slice(&u64::from(handle).to_le_bytes());
    reply[44..48].copy_from_slice(&160u32.to_le_bytes());
    // No guest tracing controller has enabled this dormant registration.
    reply[112..160].fill(0);
    // Full probes above guarantee stable writable spans. Preserve native copy
    // order, including ReturnLength overwriting a reply handle when aliased.
    if let Err(fault) = c.mem().wr(output, &reply) {
        let _ = c.p.objects.close(u64::from(handle));
        return Err(fault.into());
    }
    c.mem().w32(returned, 160)?;
    Flow::ret(STATUS_SUCCESS.into())
}
