//! Read-only acquisition below the primary SID and the one fixed policy suffix.
use super::*;

pub(crate) fn capture(registry: Registry, sid: &[u8]) -> io::Result<Registry> {
    let fixed = super::super::user_policy_path(sid)?;
    let tree = tree::capture_path_with(&registry, fixed.clone(), |path| open(&fixed, path))?;
    registry.with_user_policy(sid, tree)
}

pub(super) fn open(fixed: &[u16], path: &[u16]) -> io::Result<OwnedKey> {
    if path != fixed && (!path.starts_with(fixed) || path.get(fixed.len()) != Some(&92)) {
        return Err(invalid(
            "native per-user path outside fixed runtime selection",
        ));
    }
    if path.contains(&0) {
        return Err(invalid("native per-user path contains NUL"));
    }
    let prefix: Vec<_> = "\\Registry\\User\\".encode_utf16().collect();
    let suffix = path
        .strip_prefix(prefix.as_slice())
        .ok_or_else(|| invalid("fixed runtime path lacks native HKU root"))?;
    let name: Vec<_> = suffix.iter().copied().chain(Some(0)).collect();
    let mut handle = null_mut();
    // SAFETY: the captured primary SID plus a fixed suffix and bounded native-
    // enumerated children form this terminated name. Query/enumerate access only,
    // exclusive initialized HKEY output, and no pointers survive the call.
    let status = unsafe {
        RegOpenKeyExW(
            (-2_147_483_645isize) as Hkey,
            name.as_ptr(),
            0,
            9,
            &mut handle,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status));
    }
    Ok(OwnedKey(handle))
}
