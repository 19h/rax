//! Guest-owned unnamed pagefile sections; native build29683 capture order.
//! No guest pointer, handle, name or argument is forwarded to a host NT call.
use super::object_create::{self, Attributes};
use super::query::{guard_result, probe_write};
use crate::user::mm::{PAGE_SIZE, SharedObject};
use crate::user::windows::arch::WinArch;
use crate::user::windows::hle::{ApiResult, Ctx, Flow};
use crate::user::windows::memory::Mem;
use crate::user::windows::nt::status::*;
use crate::user::windows::objects::Object;
use std::sync::Arc;

const KIND: &str = "pagefile section";
const SECTION_ALL_ACCESS: u32 = 0x000F_001F;
const SEC_RESERVE: u32 = 0x0400_0000;
const SEC_COMMIT: u32 = 0x0800_0000;
const SEC_IMAGE: u32 = 0x0100_0000;
const SEC_NOCACHE: u32 = 0x1000_0000;
const SEC_WRITECOMBINE: u32 = 0x4000_0000;
const OUTER_ALLOCATION_BITS: u32 = 0xDD6E_007F;
const SEC_LARGE_PAGES: u32 = 0x8000_0000;
const MAX_BACKING_BYTES: u64 = 64 << 20;

pub(super) fn create(c: &mut Ctx) -> ApiResult {
    let result = checked(c);
    guard_result(c, result)
}
struct Request {
    out: u64,
    access: u32,
    object: u64,
    maximum: u64,
    protection: u32,
    attributes: u32,
    file: u64,
}
fn checked(c: &mut Ctx) -> ApiResult {
    let request = Request {
        out: c.ptr(0)?,
        access: c.u32(1)?,
        object: c.ptr(2)?,
        maximum: c.ptr(3)?,
        protection: c.u32(4)?,
        attributes: c.u32(5)?,
        file: c.ptr(6)?,
    };
    let out = request.out;
    let wow = c.arch() == WinArch::X86;
    let converted = if wow {
        Some(match object_create::capture(c, request.object, KIND)? {
            Ok(a) => a,
            Err(s) => return Flow::ret(s.into()),
        })
    } else {
        None
    };
    // WoW64 converts OBJECT_ATTRIBUTES before entering the native service.
    // Initial mode/PAGE syntax checks then precede the caller-buffer probes.
    // PAGE=0 is deferred to the later memory-manager validation, unlike other
    // invalid PAGE values; native-priority and valid-cross controls bind this.
    let attributes = request.attributes;
    let result = preflight_attributes(attributes).or_else(|| {
        (request.protection != 0 && !valid_section_protection(request.protection))
            .then_some(STATUS_INVALID_PAGE_PROTECTION)
    });
    if let Some(status) = result {
        let result = Flow::ret(status.into());
        if out != 0 {
            object_create::wow_failure(c, out, &result)?;
        }
        return result;
    }
    probe_write(c, out, c.psize() as usize)?;
    let result = construct(c, request, converted);
    object_create::wow_failure(c, out, &result)?;
    result
}
fn valid_section_protection(protection: u32) -> bool {
    matches!(protection, 2 | 4 | 8 | 0x10 | 0x20 | 0x40 | 0x80)
}
fn preflight_attributes(attributes: u32) -> Option<u32> {
    let mode = attributes & (SEC_RESERVE | SEC_COMMIT | SEC_IMAGE);
    if attributes & !OUTER_ALLOCATION_BITS != 0
        || !matches!(mode, SEC_RESERVE | SEC_COMMIT | SEC_IMAGE)
        || (mode == SEC_IMAGE && attributes & !(SEC_IMAGE | SEC_NOCACHE | 1) != 0)
    {
        return Some(STATUS_INVALID_PARAMETER_6);
    }
    None
}
fn construct(c: &mut Ctx, request: Request, converted: Option<Attributes>) -> ApiResult {
    let Request {
        out,
        access,
        object,
        maximum,
        protection,
        attributes,
        file,
    } = request;
    let size = if maximum == 0 {
        0
    } else {
        if maximum % 8 != 0 {
            return Flow::ret(STATUS_DATATYPE_MISALIGNMENT.into());
        }
        c.mem().u64(maximum)?
    };
    let mode = attributes & (SEC_RESERVE | SEC_COMMIT | SEC_IMAGE);
    if attributes & 0x0006_0000 != 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    if !valid_section_protection(protection) {
        return Flow::ret(STATUS_INVALID_PAGE_PROTECTION.into());
    }
    // WC alone rejects executable sections after maximum-size capture.
    // With both cache flags, NOCACHE wins and the section reports no WC bit.
    if attributes & (SEC_NOCACHE | SEC_WRITECOMBINE) == SEC_WRITECOMBINE && protection & 0xF0 != 0 {
        return Flow::ret(STATUS_INVALID_PAGE_PROTECTION.into());
    }
    if attributes & 0x7E != 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    if file != 0 {
        return match c.p.objects.get(file) {
            Some(Object::File(_)) => {
                Err(c.unsupported("NtCreateSection for a file-backed section"))
            }
            Some(_) => Flow::ret(STATUS_OBJECT_TYPE_MISMATCH.into()),
            None if file == c.arch().ptr(u64::MAX) || file == c.arch().ptr(u64::MAX - 1) => {
                Flow::ret(STATUS_OBJECT_TYPE_MISMATCH.into())
            }
            None => Flow::ret(STATUS_INVALID_HANDLE.into()),
        };
    }
    if mode == SEC_IMAGE {
        return Flow::ret(STATUS_INVALID_FILE_FOR_SECTION.into());
    }
    if mode == SEC_RESERVE && attributes & SEC_LARGE_PAGES != 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER_6.into());
    }
    if size == 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER_4.into());
    }
    if (size as i64) < 0 {
        return Flow::ret(STATUS_SECTION_TOO_BIG.into());
    }
    let attrs = match converted {
        Some(a) => a,
        None => match object_create::capture(c, object, KIND)? {
            Ok(a) => a,
            Err(s) => return Flow::ret(s.into()),
        },
    };
    if attrs.flags & !0x11EF2 != 0 {
        return Flow::ret(STATUS_INVALID_PARAMETER.into());
    }
    object_create::supported_attributes(c, &attrs, KIND)?;
    if attrs.root != 0 {
        return Err(c.unsupported("NT pagefile section root handle without a name"));
    }
    if attrs.flags & 0x10 != 0 {
        return Err(c.unsupported("NT pagefile section OBJ_PERMANENT lifetime"));
    }
    if attributes & SEC_LARGE_PAGES != 0 {
        return Err(c.unsupported("NtCreateSection large-page allocation contract"));
    }
    if attributes & 0x0068_0000 != 0 {
        return Err(
            c.unsupported("NtCreateSection based, 64K-page or no-change allocation contract")
        );
    }
    if access & 0x0100_0000 != 0 {
        return Err(c.unsupported("NT pagefile section ACCESS_SYSTEM_SECURITY privilege"));
    }
    if size > MAX_BACKING_BYTES {
        return Err(c.unsupported("NtCreateSection exceeds the 64 MiB backing bound"));
    }
    let mut grant = access & 0x001F_001F;
    for (generic, specific) in [
        (0x8000_0000, 0x0002_0005),
        (0x4000_0000, 0x0002_0002),
        (0x2000_0000, 0x0002_0008),
        (0x1000_0000, SECTION_ALL_ACCESS),
        (0x0200_0000, SECTION_ALL_ACCESS),
    ] {
        if access & generic != 0 {
            grant |= specific;
        }
    }
    let size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let attributes = if attributes & SEC_NOCACHE != 0 {
        attributes & !SEC_WRITECOMBINE
    } else {
        attributes
    };
    let shared = match SharedObject::anonymous(size) {
        Ok(s) => Arc::new(s),
        Err(_) => return Flow::ret(STATUS_INSUFFICIENT_RESOURCES.into()),
    };
    object_create::publish(
        c,
        out,
        attrs.flags & 2 != 0,
        grant,
        Object::Mapping {
            file: None,
            size,
            protect: protection,
            attributes: attributes & !1,
            shared: Some(shared),
        },
    )
}
