//! File-backed image bytes and object grants, independent of guest pointers.
use super::{ImageTemplate, SectionGrantError as GrantError};
use std::fs::File;
use std::sync::Arc;

mod portable;

pub(crate) struct Backing {
    pub(crate) template: Arc<ImageTemplate>,
    pub(crate) maximum: u64,
    pub(crate) attributes: u32,
    #[cfg(windows)]
    pub(crate) source: Option<Arc<super::installed::image::Source>>,
}

#[derive(Debug)]
pub(crate) enum Error {
    Status(u32),
    Unavailable(String),
}

pub(crate) fn grant(access: u32, security: Option<u32>) -> Result<u32, GrantError> {
    if access & 0x0100_0000 != 0 {
        match security {
            Some(0) => {}
            Some(status) => return Err(GrantError::Status(status)),
            None => {
                return Err(GrantError::Unavailable(
                    "created section security privilege is not captured",
                ));
            }
        }
    }
    let mut grant = access & 0x011F_001F;
    for (generic, specific) in [
        (0x8000_0000, 0x0002_0005),
        (0x4000_0000, 0x0002_0002),
        (0x2000_0000, 0x0002_0008),
        (0x1000_0000, 0x000F_001F),
        (0x0200_0000, 0x000F_001F),
    ] {
        if access & generic != 0 {
            grant |= specific;
        }
    }
    Ok(grant)
}

pub(crate) fn acquire(
    file: &File,
    maximum: u64,
    protection: u32,
    flags: u32,
    native: bool,
) -> Result<Backing, Error> {
    #[cfg(windows)]
    if native {
        return admitted(super::installed::file_image::acquire(
            file, maximum, protection, flags,
        )?);
    }
    #[cfg(not(windows))]
    let _ = native;
    let _ = protection;
    if flags & !0x0100_0001 != 0 {
        return Err(Error::Unavailable(
            "image allocation mode outside the captured standard SEC_IMAGE profile".into(),
        ));
    }
    admitted(portable::acquire(file, maximum)?)
}

fn admitted(backing: Backing) -> Result<Backing, Error> {
    // Native v14 proves shared writable image pages alias across independently
    // created Sections. The current image VM owns private frames; accepting
    // these runs would return success with incorrect sharing semantics.
    if backing
        .template
        .regions
        .iter()
        .any(|region| matches!(region.protection & 0xFF, 4 | 0x40))
    {
        return Err(Error::Unavailable(
            "file-backed image shared writable section coherence is not implemented".into(),
        ));
    }
    Ok(backing)
}
