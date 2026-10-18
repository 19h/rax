//! Basic state is native on all ABIs; WoW64 converts owner CLIENT_ID separately.
use super::super::query::{return_length, write_guard_result};
use super::*;

pub(in super::super) fn information(c: &mut Ctx) -> ApiResult {
    let direct = c.arch() == WinArch::X86 && c.u32(1)? == 1;
    let result = checked(c);
    if direct {
        let result = match result {
            Ok(s) | Err(Failure::Status(s)) => Flow::ret(s.into()),
            Err(Failure::Api(e)) => Err(e),
        };
        match write_guard_result(c, result) {
            Err(ApiErr::Fault(_)) => Flow::ret(STATUS_ACCESS_VIOLATION.into()),
            result => result,
        }
    } else {
        finish(c, result)
    }
}

fn checked(c: &mut Ctx) -> Checked<u32> {
    let (handle, class, output, length, returned) =
        (c.ptr(0)?, c.u32(1)?, c.ptr(2)?, c.u32(3)?, c.ptr(4)?);
    if class > 1 {
        return status(STATUS_INVALID_INFO_CLASS);
    }
    if class == 1 && c.arch() == WinArch::X86 {
        return owner_wow(c, handle, output, length, returned);
    }
    let required = if class == 0 { 8 } else { 16 };
    if length != required {
        return status(STATUS_INFO_LENGTH_MISMATCH);
    }
    if output % 4 != 0 {
        return status(STATUS_DATATYPE_MISALIGNMENT);
    }
    probe_write(c, output, required as usize)?;
    if returned != 0 {
        probe_write(c, returned, 4)?;
    }
    let id = object(c, handle, true)?;
    let Some(Object::Mutex {
        owner,
        count,
        abandoned,
    }) = c.p.objects.obj(id)
    else {
        unreachable!("checked mutant")
    };
    let (owner, count, abandoned) = (*owner, *count, *abandoned);
    if class == 0 {
        c.mem().w32(output, 1u32.wrapping_sub(count))?;
        c.mem().w8(output + 4, u8::from(owner == Some(c.t.tid)))?;
        c.mem().w8(output + 5, u8::from(abandoned))?;
    } else {
        c.write_ptr(
            output,
            if owner.is_some() {
                u64::from(c.p.pid)
            } else {
                0
            },
        )?;
        c.write_ptr(output + c.psize(), owner.map_or(0, u64::from))?;
    }
    return_length(c, returned, required)?;
    Ok(0)
}

fn owner_wow(c: &mut Ctx, handle: u64, output: u64, length: u32, returned: u64) -> Checked<u32> {
    // The selected converter adds8 bytes to widen CLIENT_ID, reserves private
    // scratch before the NT call, then translates its returned byte count.
    let capture = ((u64::from(length) + 8 + 15) & !15) + 16;
    let charge = (capture + 4095) & !4095;
    if output != 0
        && (charge > c.p.vm.commit_limit() - c.p.vm.committed_bytes()
            || charge > c.p.vm.high() - c.p.vm.low())
    {
        return status(STATUS_NO_MEMORY);
    }
    let kernel = if length != 8 {
        Err(Failure::Status(STATUS_INFO_LENGTH_MISMATCH))
    } else if output == 0 {
        Err(Failure::Status(STATUS_ACCESS_VIOLATION))
    } else {
        object(c, handle, true)
    };
    let result = match kernel {
        Ok(id) => {
            let Some(Object::Mutex { owner, .. }) = c.p.objects.obj(id) else {
                unreachable!("checked mutant")
            };
            let owner = *owner;
            c.write_ptr(
                output,
                if owner.is_some() {
                    u64::from(c.p.pid)
                } else {
                    0
                },
            )?;
            c.write_ptr(output + 4, owner.map_or(0, u64::from))?;
            0
        }
        Err(Failure::Status(code)) => code,
        Err(error) => return Err(error),
    };
    return_length(c, returned, if result == 0 { 8 } else { 0xFFFF_FFF8 })?;
    Ok(result)
}
