//! Select a container's session alias without inventing a second directory.
use super::*;

pub(super) fn session_base_link(parent: &[u16], name: &[u16], session: u32) -> bool {
    session != 0 && is(parent, &format!("\\Sessions\\{session}")) && is(name, "BaseNamedObjects")
}

/// Windows Server containers expose the session BNO as a link to the already
/// captured global directory. Preserve that link's type, target and grants;
/// opening it as another directory would misrepresent the captured parent.
pub(super) fn captured_session_alias(
    selected: &[SelectedDirectory],
    path: &[u16],
    session: u32,
) -> io::Result<bool> {
    if session == 0 || !is(path, &format!("\\Sessions\\{session}\\BaseNamedObjects")) {
        return Ok(false);
    }
    let Some(parent) = selected
        .iter()
        .find(|directory| is(&directory.path, &format!("\\Sessions\\{session}")))
    else {
        return Ok(false);
    };
    let Some(entry) = parent
        .entries
        .iter()
        .find(|entry| is(&entry.name, "BaseNamedObjects"))
    else {
        return Ok(false);
    };
    if !is(&entry.kind, "SymbolicLink") {
        return Ok(false);
    }
    let captured_global = selected
        .iter()
        .any(|directory| is(&directory.path, "\\BaseNamedObjects"));
    if entry
        .target
        .as_ref()
        .is_none_or(|target| !is(target, "\\BaseNamedObjects"))
        || entry.link_access.is_none()
        || !captured_global
    {
        return Err(invalid(
            "session BaseNamedObjects alias is outside the captured global directory",
        ));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::super::base_named_object_tests::{captures, table};
    use super::*;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    fn container_capture(session: u32) -> Vec<SelectedDirectory> {
        let mut selected = captures(session);
        selected.pop();
        let entry = &mut selected.last_mut().unwrap().entries[0];
        entry.kind = units("SymbolicLink");
        entry.target = Some(units("\\BaseNamedObjects"));
        entry.link_access = Some(LinkAccess {
            maximum_access: 0x0002_0001,
            system_security_status: STATUS_PRIVILEGE_NOT_HELD,
        });
        selected
    }

    #[test]
    fn container_session_alias_retains_native_link_and_directory_identity() {
        for session in [1, 34316, u32::MAX] {
            let selected = container_capture(session);
            let path = units(&format!("\\Sessions\\{session}\\BaseNamedObjects"));
            assert!(captured_session_alias(&selected, &path, session).unwrap());
            let namespace =
                Namespace::selected_for_session(table(), selected, Some(session)).unwrap();
            let global = namespace
                .lookup(None, &units("\\BaseNamedObjects"), 0)
                .unwrap();
            let alias = namespace.lookup(None, &path, 0).unwrap();
            assert!(Arc::ptr_eq(&global, &alias));
            let link = namespace.lookup_link(None, &path, 0).unwrap();
            assert_eq!(link.target, units("\\BaseNamedObjects"));
            assert_eq!(link.grant(1, false), Ok(1));
            assert_eq!(link.grant(2, false), Err(STATUS_ACCESS_DENIED));
            assert_eq!(
                link.grant(0x0100_0000, false),
                Err(STATUS_PRIVILEGE_NOT_HELD)
            );
            assert_eq!(
                namespace.lookup(None, &path, 0x1000).unwrap_err(),
                LookupError::Status(STATUS_REPARSE_POINT_ENCOUNTERED)
            );
        }
    }

    #[test]
    fn session_alias_selection_is_confined_to_captured_global_target() {
        let path = units("\\Sessions\\42\\BaseNamedObjects");
        for target in [
            None,
            Some("\\Outside"),
            Some("\\Sessions\\1\\BaseNamedObjects"),
            Some("\\BaseNamedObjects\\Child"),
            Some("\\basenamedobjects"),
        ] {
            let mut selected = container_capture(42);
            selected.last_mut().unwrap().entries[0].target = target.map(units);
            assert!(captured_session_alias(&selected, &path, 42).is_err());
        }
        let mut selected = container_capture(42);
        selected.last_mut().unwrap().entries[0].link_access = None;
        assert!(captured_session_alias(&selected, &path, 42).is_err());
        let mut selected = container_capture(42);
        selected.retain(|directory| !is(&directory.path, "\\BaseNamedObjects"));
        assert!(captured_session_alias(&selected, &path, 42).is_err());
    }

    #[test]
    fn ordinary_session_directories_and_other_links_are_not_alias_selections() {
        let path = units("\\Sessions\\42\\BaseNamedObjects");
        assert!(!captured_session_alias(&captures(42), &path, 42).unwrap());
        assert!(!captured_session_alias(&container_capture(42), &path, 1).unwrap());
        assert!(!captured_session_alias(&container_capture(42), &path, 0).unwrap());
        assert!(
            !captured_session_alias(&container_capture(42), &units("\\BaseNamedObjects"), 42)
                .unwrap()
        );
        assert!(session_base_link(
            &units("\\Sessions\\42"),
            &units("BaseNamedObjects"),
            42
        ));
        for (parent, name, session) in [
            ("\\Sessions\\0", "BaseNamedObjects", 0),
            ("\\Sessions\\1", "BaseNamedObjects", 42),
            ("\\Sessions\\42", "Other", 42),
        ] {
            assert!(!session_base_link(&units(parent), &units(name), session));
        }
    }
}
