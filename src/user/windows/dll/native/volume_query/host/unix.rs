//! A mapped Windows drive is a virtual mounted disk, not a Unix device code.
use crate::user::windows::fs::FileIdentity;
use crate::user::windows::objects::FileObj;

pub(super) fn device(file: &FileObj) -> Result<(u32, u32), String> {
    let metadata = file
        .host
        .as_ref()
        .map_or_else(
            || std::fs::metadata(&file.host_path),
            std::fs::File::metadata,
        )
        .map_err(|e| format!("retained mapped file metadata: {e}"))?;
    if file.host.is_none()
        && FileIdentity::of(&file.host_path, &metadata)
            .map_err(|e| format!("mapped file identity: {e}"))?
            != file.lifetime.identity
    {
        return Err("metadata-only handle pathname was replaced".into());
    }
    if !metadata.is_file() && !metadata.is_dir() {
        return Err("mapped Windows volume requires a regular file or directory".into());
    }
    // Captured Windows mounted-local-disk profile for the emulated DriveMap.
    // Real Windows files use their owning volume's actual device properties.
    Ok((7, 0x20020))
}
