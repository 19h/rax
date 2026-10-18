//! Capture precedes caller publication, including aliased attributes/output.
use super::*;

#[derive(Default)]
pub(super) struct Attributes {
    pub(super) root: u64,
    pub(super) flags: u32,
    pub(super) name: Option<Vec<u16>>,
    security: u64,
    qos: u64,
}

fn name(c: &Ctx, pointer: u64, wow: bool) -> Checked<Option<Vec<u16>>> {
    if pointer == 0 {
        return Ok(None);
    }
    if !wow {
        c.mem().bytes(pointer, 16)?;
    }
    let length = c.mem().u16(pointer)? as usize;
    if wow {
        c.mem().u16(pointer + 2)?;
    }
    let buffer = c.read_ptr(pointer + c.psize())?;
    if length != 0 && buffer % 2 != 0 {
        return status(STATUS_DATATYPE_MISALIGNMENT);
    }
    if length % 2 != 0 || length >= 65534 {
        return status(STATUS_OBJECT_NAME_INVALID);
    }
    let bytes = c.mem().bytes(buffer, length)?;
    Ok(Some(
        bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect(),
    ))
}

pub(super) fn capture(c: &Ctx, address: u64, wow: bool) -> Checked<Option<Attributes>> {
    if address == 0 {
        return Ok(None);
    }
    let size = if wow { 24 } else { 48 };
    if !wow && address % 8 != 0 {
        return status(STATUS_DATATYPE_MISALIGNMENT);
    }
    if wow && c.mem().u32(address)? != size {
        return status(STATUS_INVALID_PARAMETER);
    }
    if !wow {
        c.mem().bytes(address, size as usize)?;
    }
    if c.mem().u32(address)? != size {
        return status(STATUS_INVALID_PARAMETER);
    }
    let width = c.psize();
    let root = c.read_ptr(address + width)?;
    let np = c.read_ptr(address + 2 * width)?;
    let converted_name = if wow { Some(name(c, np, true)?) } else { None };
    let flags = c.mem().u32(address + 3 * width)?;
    let security = c.read_ptr(address + if wow { 16 } else { 32 })?;
    let qos = c.read_ptr(address + if wow { 20 } else { 40 })?;
    if !wow && flags & !0x1EF2 != 0 {
        return status(STATUS_INVALID_PARAMETER);
    }
    for (pointer, size) in [(qos, 12), (security, 20)] {
        if pointer != 0 {
            if !wow && pointer % 4 != 0 {
                return status(STATUS_DATATYPE_MISALIGNMENT);
            }
            c.mem().bytes(pointer, size)?;
        }
    }
    let name = match converted_name {
        Some(name) => name,
        None => name(c, np, false)?,
    };
    Ok(Some(Attributes {
        root,
        flags,
        name,
        security,
        qos,
    }))
}

pub(super) fn supported(c: &Ctx, attrs: &Attributes) -> Checked<()> {
    if attrs.security != 0 {
        return Err(c
            .unsupported("NT mutant explicit security descriptor")
            .into());
    }
    if attrs.qos != 0 {
        return Err(c
            .unsupported("NT mutant security quality of service")
            .into());
    }
    Ok(())
}
