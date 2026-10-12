//! Bounded, immutable selection of the installed NT object namespace.
//! NT UTF-16 names retain their protocol grammar; they are not filesystem paths.
//! Host handles never survive selection, and guest lookups never call the host.
use super::nt::status::*;
use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;
pub(crate) const PATHS: [&str; 3] = ["\\", "\\KnownDlls", "\\KnownDlls32"];
pub(crate) const MAX_ENTRIES: usize = 8192;
const MAX_UNITS: usize = 32766;
const MAX_TOTAL_BYTES: usize = 16 << 20;
const ALL_ACCESS: u32 = 0x000F_000F;
#[derive(Debug, Clone)]
pub(crate) struct CapturedEntry {
    pub(crate) name: Vec<u16>,
    pub(crate) kind: Vec<u16>,
    pub(crate) target: Option<Vec<u16>>,
}
#[derive(Debug)]
pub(crate) struct SelectedDirectory {
    pub(crate) path: Vec<u16>,
    pub(crate) maximum_access: u32,
    pub(crate) force_case_insensitive: bool,
    pub(crate) system_security_status: u32,
    pub(crate) entries: Vec<CapturedEntry>,
}
#[derive(Debug)]
struct Entry {
    captured: CapturedEntry,
    child: Option<Arc<Directory>>,
}
/// Guest-owned immutable directory metadata, independent of native HANDLEs.
#[derive(Debug)]
pub struct Directory {
    pub(crate) path: Vec<u16>,
    pub(crate) maximum_access: u32,
    system_security_status: u32,
    force_case_insensitive: bool,
    upcase: Arc<[u16]>,
    entries: BTreeMap<Vec<u16>, Entry>,
}
#[derive(Debug, Clone, Default)]
pub(crate) struct Namespace {
    root: Option<Arc<Directory>>,
    pub(crate) wow_system_prefix: Vec<u16>,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LookupError {
    Status(u32),
    Unselected,
}
fn invalid(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s)
}
fn fold(upcase: &[u16], s: &[u16]) -> Vec<u16> {
    s.iter().map(|&u| upcase[usize::from(u)]).collect()
}
fn is(s: &[u16], text: &str) -> bool {
    s.iter().copied().eq(text.encode_utf16())
}
impl Namespace {
    /// O(E log E + U) time and O(E + U) retained storage for E entries and U UTF-16 units.
    pub(crate) fn selected(
        upcase: Arc<[u16]>,
        selected: Vec<SelectedDirectory>,
    ) -> io::Result<Self> {
        if upcase.len() != 65536 || selected.is_empty() || selected.len() > PATHS.len() {
            return Err(invalid("NT directory selection/table exceeds bounds"));
        }
        let mut dirs = BTreeMap::new();
        let (mut count, mut bytes) = (0usize, 0usize);
        for s in selected {
            if !PATHS.iter().any(|p| is(&s.path, p))
                || s.maximum_access & !ALL_ACCESS != 0
                || !matches!(
                    s.system_security_status,
                    0 | STATUS_PRIVILEGE_NOT_HELD | STATUS_ACCESS_DENIED
                )
            {
                return Err(invalid("NT directory outside fixed selection/access mask"));
            }
            let mut entries = BTreeMap::new();
            for e in s.entries {
                if e.name.is_empty()
                    || e.name.len() > MAX_UNITS
                    || e.name.contains(&(b'\\' as u16))
                    || e.name.contains(&0)
                    || e.kind.is_empty()
                    || e.kind.len() > 255
                    || e.kind.contains(&0)
                    || e.target
                        .as_ref()
                        .is_some_and(|t| t.len() > MAX_UNITS || t.contains(&0))
                    || e.target.is_some() && !is(&e.kind, "SymbolicLink")
                {
                    return Err(invalid(
                        "NT directory entry exceeds its name/type/target bounds",
                    ));
                }
                count = count
                    .checked_add(1)
                    .ok_or_else(|| invalid("NT directory count overflow"))?;
                bytes = bytes
                    .checked_add(
                        2 * (e.name.len() + e.kind.len() + e.target.as_ref().map_or(0, Vec::len)),
                    )
                    .ok_or_else(|| invalid("NT directory byte count overflow"))?;
                if count > MAX_ENTRIES || bytes > MAX_TOTAL_BYTES {
                    return Err(invalid("NT directory aggregate exceeds bounds"));
                }
                if entries
                    .insert(
                        fold(&upcase, &e.name),
                        Entry {
                            captured: e,
                            child: None,
                        },
                    )
                    .is_some()
                {
                    return Err(invalid("colliding NT directory names"));
                }
            }
            let key = s.path.clone();
            let dir = Directory {
                path: s.path,
                maximum_access: s.maximum_access,
                system_security_status: s.system_security_status,
                force_case_insensitive: s.force_case_insensitive,
                upcase: upcase.clone(),
                entries,
            };
            if dirs.insert(key, dir).is_some() {
                return Err(invalid("duplicate NT directory selection"));
            }
        }
        let mut root = dirs
            .remove(&vec![b'\\' as u16])
            .ok_or_else(|| invalid("NT directory root is not selected"))?;
        for (path, dir) in dirs {
            let name = &path[1..];
            let entry = root
                .entries
                .get_mut(&fold(&upcase, name))
                .ok_or_else(|| invalid("selected NT directory is absent from captured root"))?;
            if !is(&entry.captured.kind, "Directory") || entry.captured.name != name {
                return Err(invalid("selected NT directory root type/name changed"));
            }
            entry.child = Some(Arc::new(dir));
        }
        Ok(Self {
            root: Some(Arc::new(root)),
            wow_system_prefix: Vec::new(),
        })
    }
    pub(crate) fn select_wow_system_root(&mut self, root: &[u16]) -> io::Result<()> {
        if root.is_empty() || root.len() > 32752 || root.contains(&0) {
            return Err(invalid("selected WoW64 system root exceeds bounds"));
        }
        // This is the NT protocol prefix used by the installed redirection
        // pass, rather than a native filesystem path to be normalized.
        self.wow_system_prefix = "\\??\\"
            .encode_utf16()
            .chain(root.iter().copied())
            .collect();
        Ok(())
    }
    pub(crate) fn equal_unit(&self, a: u16, b: u16, insensitive: bool) -> bool {
        if insensitive {
            self.root
                .as_ref()
                .is_some_and(|r| r.upcase[usize::from(a)] == r.upcase[usize::from(b)])
        } else {
            a == b
        }
    }
    pub(crate) fn lookup(
        &self,
        root: Option<Arc<Directory>>,
        name: &[u16],
        flags: u32,
    ) -> Result<Arc<Directory>, LookupError> {
        let (mut dir, path) = if let Some(root) = root {
            if name.first() == Some(&(b'\\' as u16)) {
                return Err(LookupError::Status(STATUS_OBJECT_PATH_SYNTAX_BAD));
            }
            (root, name)
        } else {
            if name.first() != Some(&(b'\\' as u16)) {
                return Err(LookupError::Status(STATUS_OBJECT_PATH_SYNTAX_BAD));
            }
            (
                self.root.as_ref().ok_or(LookupError::Unselected)?.clone(),
                &name[1..],
            )
        };
        if path.is_empty() {
            return Ok(dir);
        }
        let mut parts = path.split(|&u| u == b'\\' as u16).peekable();
        while let Some(part) = parts.next() {
            if part.is_empty() {
                return Err(LookupError::Status(STATUS_OBJECT_NAME_INVALID));
            }
            let entry = dir
                .entries
                .get(&fold(&dir.upcase, part))
                .filter(|e| {
                    dir.force_case_insensitive || flags & 0x40 != 0 || e.captured.name == part
                })
                .ok_or(LookupError::Status(if parts.peek().is_some() {
                    STATUS_OBJECT_PATH_NOT_FOUND
                } else {
                    STATUS_OBJECT_NAME_NOT_FOUND
                }))?;
            if is(&entry.captured.kind, "Directory") {
                dir = entry.child.as_ref().ok_or(LookupError::Unselected)?.clone();
            } else if is(&entry.captured.kind, "SymbolicLink") {
                if flags & 0x1000 != 0 {
                    return Err(LookupError::Status(STATUS_REPARSE_POINT_ENCOUNTERED));
                }
                let target = entry
                    .captured
                    .target
                    .as_ref()
                    .ok_or(LookupError::Unselected)?;
                // The selected KnownDllPath targets are native DOS paths. Object-manager
                // parsing rejects them; this is not a filesystem translation operation.
                return Err(if target.first() != Some(&(b'\\' as u16)) {
                    LookupError::Status(STATUS_OBJECT_NAME_INVALID)
                } else {
                    LookupError::Unselected
                });
            } else {
                return Err(LookupError::Status(STATUS_OBJECT_TYPE_MISMATCH));
            }
        }
        Ok(dir)
    }
}
impl Directory {
    /// Shared mapping plus the selected caller's captured authorization ceiling.
    /// Duplication permits zero rights and discards SYNCHRONIZE/MAXIMUM_ALLOWED;
    /// opening applies MAXIMUM_ALLOWED and refuses a zero grant.
    pub(crate) fn grant(&self, access: u32, duplicate: bool) -> Result<u32, u32> {
        let mut requested = access & 0x0DFF_FFFF;
        if duplicate {
            requested &= !0x0010_0000;
        }
        for (generic, specific) in [
            (0x8000_0000, 0x0002_0003),
            (0x4000_0000, 0x0002_000C),
            (0x2000_0000, 0x0002_0003),
            (0x1000_0000, ALL_ACCESS),
        ] {
            if access & generic != 0 {
                requested |= specific;
            }
        }
        if requested & !(ALL_ACCESS | 0x0100_0000) != 0
            || requested & ALL_ACCESS & !self.maximum_access != 0
        {
            return Err(STATUS_ACCESS_DENIED);
        }
        if requested & 0x0100_0000 != 0 && self.system_security_status != 0 {
            return Err(self.system_security_status);
        }
        if !duplicate && access & 0x0200_0000 != 0 {
            requested |= self.maximum_access;
        }
        if !duplicate && requested == 0 {
            return Err(STATUS_ACCESS_DENIED);
        }
        Ok(requested)
    }
}
#[cfg(windows)]
mod installed;
#[cfg(windows)]
pub(crate) use installed::snapshot;
#[cfg(not(windows))]
pub(crate) fn snapshot(_: Arc<[u16]>) -> io::Result<Namespace> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "installed NT object namespace requires a Windows host",
    ))
}
#[cfg(test)]
mod tests;
