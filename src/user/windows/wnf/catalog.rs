//! Build29683 name validation and bounded classic-DACL admission.
use crate::user::windows::registry::Value;
pub(super) use crate::user::windows::security::{Group, Subject};
use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;

const XOR_KEY: u64 = 0x41C6_4E6D_A3BC_0074;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Name {
    pub(crate) lifetime: u8,
    pub(crate) scope: u8,
}
impl Name {
    pub(crate) fn decode(value: u64) -> Option<Self> {
        let bits = value ^ XOR_KEY;
        let scope = ((bits >> 6) & 15) as u8;
        let persistent = bits & 0x400 != 0;
        if bits & 15 != 1 || scope > 5 || (persistent && !matches!(scope, 0 | 2 | 4)) {
            return None;
        }
        Some(Self {
            lifetime: ((bits >> 4) & 3) as u8,
            scope,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Permission {
    Missing,
    Denied,
    Unmodeled,
    Allowed,
}
#[derive(Debug, Clone, Default)]
pub(crate) struct Catalog {
    records: Arc<BTreeMap<u64, Option<u32>>>,
    primary_subject: Option<Arc<Subject>>,
    primary_token_access: Option<crate::user::windows::security::token::Access>,
    primary_token_session_id: Option<u32>,
    primary_token_scalars: Option<crate::user::windows::security::token::scalars::Scalars>,
    queries: Option<Arc<super::Queries>>,
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn u16le(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}
fn u32le(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}
pub(super) fn sid(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.first() != Some(&1) || *bytes.get(1)? > 15 {
        return None;
    }
    bytes.get(..8 + usize::from(bytes[1]) * 4)
}
pub(super) fn matches(subject: &Subject, candidate: &[u8], deny: bool, restricted: bool) -> bool {
    if !restricted && subject.user == candidate {
        return true;
    }
    let groups = if restricted {
        &subject.restricted
    } else {
        &subject.groups
    };
    groups.iter().any(|group| {
        group.sid == candidate
            && if deny {
                group.attributes & (4 | 16) != 0
            } else {
                group.attributes & 4 != 0 && group.attributes & 16 == 0
            }
    })
}
fn grants(sd: &[u8], subject: &Subject) -> Option<u32> {
    // This profile admits an absent owner/group/SACL and classic, noninherited
    // allow/deny ACEs. Conditional/object ACEs, labels, AppContainers and other
    // descriptor controls remain explicit unmodeled inputs, never grant-all.
    if subject.app_container
        || !subject.restricted.is_empty()
        || sd.len() < 28
        || sd[0] != 1
        || sd[1] != 0
        || u16le(sd, 2)? != 0x8004
        || (4..16).any(|at| sd[at] != 0)
        || u32le(sd, 16)? != 20
    {
        return None;
    }
    let acl = &sd[20..];
    if !matches!(acl[0], 2 | 4)
        || acl[1] != 0
        || u16le(acl, 2)? as usize != acl.len()
        || u16le(acl, 6)? != 0
    {
        return None;
    }
    let mut entries = Vec::new();
    let mut at = 8;
    for _ in 0..u16le(acl, 4)? {
        let kind = *acl.get(at)?;
        let size = u16le(acl, at + 2)? as usize;
        if !matches!(kind, 0 | 1) || *acl.get(at + 1)? != 0 || size < 16 || size % 4 != 0 {
            return None;
        }
        let ace = acl.get(at..at.checked_add(size)?)?;
        let mask = u32le(ace, 4)?;
        let candidate = sid(&ace[8..])?;
        if candidate.len() + 8 != ace.len() {
            return None;
        }
        if mask & !3 != 0 && matches(subject, candidate, kind == 1, false) {
            return None;
        }
        entries.push((kind, mask, candidate));
        at += size;
    }
    if at != acl.len() {
        return None;
    }
    let allowed = |bit: u32, restricted: bool| {
        for &(kind, mask, candidate) in &entries {
            if mask & bit != 0 && matches(subject, candidate, kind == 1, restricted) {
                return kind == 0;
            }
        }
        false
    };
    Some(
        [1, 2]
            .into_iter()
            .filter(|&bit| {
                allowed(bit, false) && (subject.restricted.is_empty() || allowed(bit, true))
            })
            .fold(0, |mask, bit| mask | bit),
    )
}
impl Catalog {
    /// O(V log V + A*S) time and O(V) retained records; V<=4096 values,
    /// A total ACE count, S captured token SID count. No host pointer is retained.
    pub(crate) fn selected(values: Vec<Value>, subject: Subject) -> io::Result<Self> {
        if values.len() > 4096
            || subject.privileges.len() > 4096
            || sid(&subject.user) != Some(subject.user.as_slice())
            || subject
                .groups
                .iter()
                .chain(&subject.restricted)
                .any(|g| sid(&g.sid) != Some(g.sid.as_slice()))
        {
            return Err(invalid("WNF catalog/subject exceeds its bounded schema"));
        }
        let mut records = BTreeMap::new();
        for value in values {
            if value.kind != 3 || value.name.len() != 16 || value.data.len() < 4 {
                return Err(invalid(
                    "installed WNF value is not a named binary descriptor",
                ));
            }
            let name = value
                .name
                .iter()
                .try_fold(0u64, |n, &unit| {
                    char::from_u32(u32::from(unit))?
                        .to_digit(16)
                        .map(|digit| (n << 4) | u64::from(digit))
                })
                .ok_or_else(|| invalid("installed WNF name is not 16 hexadecimal units"))?;
            let parsed =
                Name::decode(name).ok_or_else(|| invalid("installed WNF name is malformed"))?;
            if parsed.lifetime != 0 || records.contains_key(&name) {
                return Err(invalid(
                    "installed WNF catalog is duplicate or not well-known",
                ));
            }
            let length = value.data.len();
            let maximum = u32le(&value.data, length - 4).unwrap();
            let grant = if parsed.scope == 3 && maximum <= 4096 {
                grants(&value.data[..length - 4], &subject)
            } else if parsed.scope == 0 {
                super::Definition::parse(&value)
                    .ok()
                    .and_then(|definition| {
                        let trailer = if definition.type_id.is_some() { 20 } else { 4 };
                        super::system::grants(&value.data[..length - trailer], &subject)
                    })
            } else {
                None
            };
            records.insert(name, grant);
        }
        // A publish-only grant is not represented by the recorded provisioned
        // process catalog. Its mask conversion remains unmodeled rather than
        // applying an unverified access requirement.
        for (name, grant) in records.iter_mut() {
            if Name::decode(*name).is_some_and(|parsed| parsed.scope == 3) && *grant == Some(2) {
                *grant = None;
            }
        }
        Ok(Self {
            records: Arc::new(records),
            primary_subject: Some(Arc::new(subject)),
            primary_token_access: None,
            primary_token_session_id: None,
            primary_token_scalars: None,
            queries: None,
        })
    }
    pub(crate) fn with_queries(mut self, queries: super::Queries) -> Self {
        self.queries = Some(Arc::new(queries));
        self
    }
    pub(crate) fn with_primary_token_access(
        mut self,
        access: crate::user::windows::security::token::Access,
    ) -> Self {
        self.primary_token_access = Some(access);
        self
    }
    pub(crate) fn primary_token_access(
        &self,
    ) -> Option<crate::user::windows::security::token::Access> {
        self.primary_token_access
    }
    pub(crate) fn queries(&self) -> Option<Arc<super::Queries>> {
        self.queries.clone()
    }

    pub(crate) fn with_primary_token_session_id(mut self, session: u32) -> Self {
        self.primary_token_session_id = Some(session);
        self
    }

    pub(crate) fn with_primary_token_scalars(
        mut self,
        values: crate::user::windows::security::token::scalars::Scalars,
    ) -> Self {
        self.primary_token_scalars = Some(values);
        self
    }

    pub(crate) fn primary_token_scalar(
        &self,
        class: u32,
    ) -> Option<crate::user::windows::security::token::scalars::CapturedScalar> {
        self.primary_token_scalars
            .as_ref()
            .and_then(|values| values.get(class))
    }

    pub(crate) fn primary_token_session_id(&self) -> Option<u32> {
        self.primary_token_session_id
    }
    pub(crate) fn subscription_stamp(&self, name: u64) -> Result<Option<u32>, &'static str> {
        let queries = self
            .queries
            .as_ref()
            .ok_or("System WNF subscription requires selected query metadata")?;
        match queries.get(name).map(|(_, record)| record) {
            Some(super::Record::Data(data)) if data.stamp != 0 => Ok(Some(data.stamp)),
            Some(super::Record::Data(data)) if data.bytes.is_empty() => Ok(None),
            Some(super::Record::Status(
                crate::user::windows::nt::status::STATUS_OBJECT_NAME_NOT_FOUND,
            )) => Ok(None),
            Some(super::Record::Unavailable(reason)) => Err(reason),
            _ => Err("System WNF data admission is outside the selected subscription profile"),
        }
    }
    /// Retains the same validated primary-token property used for WNF grants.
    /// `None` is an absent snapshot, not a non-AppContainer token.
    pub(crate) fn primary_app_container(&self) -> Option<bool> {
        self.primary_subject
            .as_ref()
            .map(|subject| subject.app_container)
    }
    pub(crate) fn primary_subject(&self) -> Option<Arc<Subject>> {
        self.primary_subject.clone()
    }
    pub(crate) fn permission(&self, name: u64, event_mask: u32) -> Permission {
        let Some(grant) = self.records.get(&name) else {
            return Permission::Missing;
        };
        let Some(grant) = grant else {
            return Permission::Unmodeled;
        };
        if Name::decode(name).is_some_and(|parsed| parsed.scope == 0) && self.queries.is_none() {
            return Permission::Unmodeled;
        }
        let required = u32::from(event_mask & 0x11 != 0) | (u32::from(event_mask & 0x0E != 0) << 1);
        if grant & required == required {
            Permission::Allowed
        } else {
            Permission::Denied
        }
    }
}
