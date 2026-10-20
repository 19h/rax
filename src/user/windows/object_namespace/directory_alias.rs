//! One current-session alias may reuse an independently selected fixed root.
//! Native acquisition proves handle identity; target text alone is insufficient.
use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;

use super::{Directory, fold, invalid, is};

pub(super) struct Alias {
    source: Vec<u16>,
    target: Vec<u16>,
}

/// O(E log E + U) validation is already complete in the caller. This bounded
/// projection uses O(U + log D) time/space for U path units and D selected roots.
pub(super) fn prepare(
    directories: &mut BTreeMap<Vec<u16>, Directory>,
    upcase: &[u16],
    session: Option<u32>,
    aliases: &[(Vec<u16>, Vec<u16>)],
) -> io::Result<Option<Alias>> {
    if aliases.is_empty() {
        return Ok(None);
    }
    let session = session
        .filter(|value| *value != 0)
        .ok_or_else(|| invalid("selected directory alias requires a nonzero current session"))?;
    let expected: Vec<u16> = format!("\\Sessions\\{session}\\BaseNamedObjects")
        .encode_utf16()
        .collect();
    if aliases.len() != 1 || aliases[0].0 != expected || !is(&aliases[0].1, "\\BaseNamedObjects") {
        return Err(invalid(
            "selected directory alias is outside its fixed identity pair",
        ));
    }
    let (source, target) = &aliases[0];
    let captured = directories
        .get(source)
        .ok_or_else(|| invalid("selected directory alias capture is absent"))?;
    let resolved = directories
        .get(target)
        .ok_or_else(|| invalid("selected directory alias target is not independently captured"))?;
    if captured.maximum_access != resolved.maximum_access
        || captured.system_security_status != resolved.system_security_status
        || captured.force_case_insensitive != resolved.force_case_insensitive
    {
        return Err(invalid(
            "selected directory alias authorization/case evidence disagrees",
        ));
    }
    let separator = source
        .iter()
        .rposition(|unit| *unit == 92)
        .expect("fixed path");
    let parent = directories
        .get(&source[..separator])
        .ok_or_else(|| invalid("selected directory alias parent is absent"))?;
    let name = &source[separator + 1..];
    let entry = parent
        .entries
        .get(&fold(upcase, name))
        .ok_or_else(|| invalid("selected directory alias is absent from captured parent"))?;
    if entry.captured.name != name
        || !is(&entry.captured.kind, "SymbolicLink")
        || entry.link.is_none()
        || entry.child.is_some()
        || entry
            .captured
            .target
            .as_ref()
            .and_then(|value| value.first())
            != Some(&92)
    {
        return Err(invalid(
            "selected directory alias has no coherent symbolic-link metadata",
        ));
    }
    let incomplete = !captured.entries_complete;
    directories.remove(source).expect("validated alias capture");
    if incomplete {
        directories
            .get_mut(target)
            .expect("validated alias target")
            .entries_complete = false;
    }
    Ok(Some(Alias {
        source: source.clone(),
        target: target.clone(),
    }))
}

impl Alias {
    pub(super) fn attach(self, root: &mut Directory, upcase: &[u16]) -> io::Result<()> {
        let target_name = &self.target[1..];
        let resolved = root
            .entries
            .get(&fold(upcase, target_name))
            .and_then(|entry| entry.child.clone())
            .ok_or_else(|| invalid("selected alias target attachment is absent"))?;
        let mut components = self.source[1..].split(|unit| *unit == 92).peekable();
        let mut parent = root;
        while let Some(component) = components.next() {
            let entry = parent
                .entries
                .get_mut(&fold(upcase, component))
                .ok_or_else(|| invalid("selected alias ancestor attachment is absent"))?;
            if components.peek().is_none() {
                entry.resolved_directory = Some(resolved);
                return Ok(());
            }
            parent = entry
                .child
                .as_mut()
                .and_then(Arc::get_mut)
                .ok_or_else(|| invalid("selected alias ancestor is not uniquely owned"))?;
        }
        Err(invalid("selected alias path is empty"))
    }
}
