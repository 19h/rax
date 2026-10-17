//! Bounded System-scope subscription admission from selected metadata.
use super::catalog::{matches, sid};
use crate::user::windows::security::Subject;

fn word(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}
fn dword(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// O(A*S) time, O(A) scratch, for at most 4096 classic ACEs and captured SIDs.
/// Only the measured untrusted mandatory label is admitted; no host call occurs.
pub(super) fn grants(sd: &[u8], subject: &Subject) -> Option<u32> {
    if subject.app_container
        || !subject.restricted.is_empty()
        || sd.len() < 28
        || sd[0] != 1
        || sd[1] != 0
        || !matches!(word(sd, 2)?, 0x8004 | 0x8014)
    {
        return None;
    }
    for at in [4, 8] {
        let offset = dword(sd, at)? as usize;
        if offset != 0 && (offset < 20 || offset % 4 != 0 || sid(sd.get(offset..)?).is_none()) {
            return None;
        }
    }
    let sacl = dword(sd, 12)? as usize;
    if word(sd, 2)? & 0x10 != 0 {
        // ACL revision 2, one mandatory-label ACE, NO_WRITE_UP, S-1-16-0.
        const UNTRUSTED: [u8; 28] = [
            2, 0, 28, 0, 1, 0, 0, 0, 17, 0, 20, 0, 1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 16, 0, 0, 0, 0,
        ];
        if sacl < 20 || sacl % 4 != 0 || sd.get(sacl..sacl.checked_add(28)?)? != UNTRUSTED {
            return None;
        }
    } else if sacl != 0 {
        return None;
    }
    let offset = dword(sd, 16)? as usize;
    if offset < 20 || offset % 4 != 0 {
        return None;
    }
    let acl = sd.get(offset..)?;
    let length = usize::from(word(acl, 2)?);
    let acl = acl.get(..length)?;
    let count = word(acl, 4)?;
    if length < 8 || !matches!(acl[0], 2 | 4) || acl[1] != 0 || word(acl, 6)? != 0 || count > 4096 {
        return None;
    }
    let mut entries = Vec::with_capacity(usize::from(count));
    let mut at = 8usize;
    for _ in 0..count {
        let kind = *acl.get(at)?;
        let size = usize::from(word(acl, at + 2)?);
        if !matches!(kind, 0 | 1) || *acl.get(at + 1)? != 0 || size < 16 || size % 4 != 0 {
            return None;
        }
        let ace = acl.get(at..at.checked_add(size)?)?;
        let candidate = sid(ace.get(8..)?)?;
        if candidate.len() + 8 != ace.len() {
            return None;
        }
        if candidate == [1, 1, 0, 0, 0, 0, 0, 3, 4, 0, 0, 0] {
            return None;
        }
        let raw = dword(ace, 4)?;
        // Standard object rights do not grant either WNF data right. Generic
        // execute and unmeasured specific bits remain unavailable.
        if raw & !(0xD000_0000 | 0x001F_0000 | 3) != 0 {
            return None;
        }
        let mut mask = raw & 3;
        if raw & 0x8000_0000 != 0 {
            mask |= 1;
        }
        if raw & 0x4000_0000 != 0 {
            mask |= 2;
        }
        if raw & 0x1000_0000 != 0 {
            mask |= 3;
        }
        entries.push((kind, mask, candidate));
        at += size;
    }
    if at != acl.len() {
        return None;
    }
    let allowed = |bit| {
        entries
            .iter()
            .find(|(kind, mask, candidate)| {
                mask & bit != 0 && matches(subject, candidate, *kind == 1, false)
            })
            .is_some_and(|(kind, _, _)| *kind == 0)
    };
    Some(u32::from(allowed(1)) | (u32::from(allowed(2)) << 1))
}
