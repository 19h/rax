//! Initial installed NTDLL shares the selected kernel image's address contract.
use super::*;
use crate::user::windows::object_namespace::{ImageSection, ImageTemplate};
#[cfg(test)]
#[path = "native_image_tests.rs"]
mod tests;

pub(super) fn map(
    p: &mut Proc,
    pe: &PeImage,
    path: Option<&Path>,
) -> Result<Option<u64>, LoadError> {
    let Some(runtime) = p.native.as_ref() else {
        return Ok(None);
    };
    let Some(path) = path else {
        return Ok(None);
    };
    let selected = runtime.dll("ntdll.dll").map_err(|e| {
        LoadError::new(
            STATUS_DLL_NOT_FOUND,
            format!("selected NTDLL identity: {e}"),
        )
    })?;
    if selected.as_deref() != Some(path) {
        return Ok(None);
    }
    let root = if p.arch == WinArch::X86 && !cfg!(target_arch = "x86") {
        "\\KnownDlls32\\ntdll.dll"
    } else {
        "\\KnownDlls\\ntdll.dll"
    };
    let section = runtime
        .object_namespace
        .lookup_section(None, &root.encode_utf16().collect::<Vec<_>>(), 0x40, false)
        .map_err(|e| {
            LoadError::new(
                STATUS_NOT_SUPPORTED,
                format!("selected NTDLL section: {e:?}"),
            )
        })?;
    let image = section
        .image_for(p.arch)
        .map_err(|e| LoadError::new(STATUS_NOT_SUPPORTED, format!("selected NTDLL image: {e}")))?;
    if image.extensions.iter().all(Option::is_none) {
        return Ok(None);
    }
    map_selected(p, pe, section, &image).map(Some)
}

fn map_selected(
    p: &mut Proc,
    pe: &PeImage,
    section: Arc<ImageSection>,
    image: &ImageTemplate,
) -> Result<u64, LoadError> {
    // File metadata still owns the module/export/unwind interpretation. Admit
    // this kernel snapshot only for that exact installed file's PE identity.
    let h = pe.headers();
    let at = h.nt_offset as usize;
    let bytes = &image.bytes;
    if image.machine != h.machine
        || image.bytes.len() != pe.mapped_size() as usize
        || bytes.get(0x3C..0x40) != Some(&(h.nt_offset).to_le_bytes())
        || bytes.get(at + 8..at + 12) != Some(&h.time_date_stamp.to_le_bytes())
        || section.size != image.bytes.len() as u64
    {
        return Err(LoadError::new(
            STATUS_INVALID_IMAGE_FORMAT,
            "installed NTDLL file and pinned section identities disagree",
        ));
    }
    let base = image.preferred;
    // Native default guard branches can reference this shared preferred base.
    // A fallback placement would silently break those cross-image branches.
    if !p.vm.is_free(base, image.allocation_size()) {
        return Err(LoadError::new(
            STATUS_CONFLICTING_ADDRESSES,
            "selected NTDLL preferred image and extension range is occupied",
        ));
    }
    p.vm.map_section_image(
        section,
        image,
        base,
        base,
        image.allocation_size(),
        0,
        image.bytes.len() as u64,
        false,
        false,
    )
    .map_err(|e| LoadError::new(e.status(), "cannot map selected NTDLL and extensions"))?;
    Ok(base)
}
