//! Bounded immutable state selected from fixed WNF provisioning metadata.
use super::Name;
use crate::user::windows::nt::status::*;
use crate::user::windows::registry::Value;
use std::collections::BTreeMap;
use std::io;
use std::sync::Arc;

pub(super) const MAX_STATE_BYTES: usize = 65536;
const MAX_SELECTED_BYTES: usize = 16 << 20;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn u16le(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}
fn u32le(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Definition {
    pub(crate) name: u64,
    pub(crate) maximum: u32,
    pub(crate) type_id: Option<[u8; 16]>,
}
impl Definition {
    /// Fixed registry grammar: relative descriptor, maximum DWORD, optional GUID.
    /// O(B) validation, O(1) scratch; a record has at most 65536 metadata bytes.
    pub(super) fn parse(value: &Value) -> io::Result<Self> {
        if value.kind != 3 || value.name.len() != 16 || value.data.len() > MAX_STATE_BYTES {
            return Err(invalid("unmodeled WNF query provisioning record"));
        }
        let name = value
            .name
            .iter()
            .try_fold(0u64, |n, &u| {
                char::from_u32(u32::from(u))?
                    .to_digit(16)
                    .map(|d| (n << 4) | u64::from(d))
            })
            .ok_or_else(|| invalid("invalid WNF query provisioning name"))?;
        if Name::decode(name).is_none_or(|parsed| parsed.lifetime != 0) {
            return Err(invalid("WNF query provisioning name is not well-known"));
        }
        let raw = &value.data;
        if raw.len() < 24 || raw[0] != 1 || u16le(raw, 2).is_none_or(|c| c & 0x8000 == 0) {
            return Err(invalid("WNF query metadata has no relative descriptor"));
        }
        let mut end = 20usize;
        for index in 0..4 {
            let at = u32le(raw, 4 + 4 * index).unwrap() as usize;
            if at == 0 {
                continue;
            }
            if at < 20 || at % 4 != 0 {
                return Err(invalid("WNF query descriptor offset is invalid"));
            }
            let bytes = if index < 2 {
                let sid = raw
                    .get(at..)
                    .and_then(super::catalog::sid)
                    .ok_or_else(|| invalid("WNF query descriptor SID is invalid"))?;
                sid.len()
            } else {
                if !matches!(raw.get(at), Some(2 | 4)) {
                    return Err(invalid("WNF query descriptor ACL revision is unmodeled"));
                }
                let bytes = usize::from(
                    u16le(raw, at + 2)
                        .ok_or_else(|| invalid("WNF query descriptor ACL is truncated"))?,
                );
                if bytes < 8 {
                    return Err(invalid("WNF query descriptor ACL extent is invalid"));
                }
                bytes
            };
            let limit = at
                .checked_add(bytes)
                .filter(|&limit| limit <= raw.len())
                .ok_or_else(|| invalid("WNF query descriptor escapes its record"))?;
            end = end.max(limit);
        }
        let trailer = raw
            .get(end..)
            .ok_or_else(|| invalid("WNF query trailer is absent"))?;
        if !matches!(trailer.len(), 4 | 20) {
            return Err(invalid("WNF query provisioning trailer is unmodeled"));
        }
        Ok(Self {
            name,
            maximum: u32le(trailer, 0).unwrap(),
            type_id: (trailer.len() == 20).then(|| trailer[4..].try_into().unwrap()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Data {
    pub(crate) stamp: u32,
    pub(crate) bytes: Arc<[u8]>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Record {
    Data(Data),
    Status(u32),
    Unavailable(&'static str),
}
#[derive(Debug, Clone)]
pub(crate) struct Queries {
    records: BTreeMap<u64, (Definition, Record)>,
    pub(crate) session: u32,
}
impl Queries {
    pub(crate) fn selected(records: Vec<(Definition, Record)>, session: u32) -> io::Result<Self> {
        if records.len() > 4096 {
            return Err(invalid("WNF query record limit exceeded"));
        }
        let mut result = BTreeMap::new();
        let mut total = 0usize;
        for (definition, record) in records {
            if Name::decode(definition.name).is_none() || result.contains_key(&definition.name) {
                return Err(invalid("WNF query record is invalid or duplicate"));
            }
            match &record {
                Record::Data(data) => {
                    if data.bytes.len() > MAX_STATE_BYTES
                        || data.bytes.len() > definition.maximum as usize
                    {
                        return Err(invalid("WNF query data exceeds its declared limit"));
                    }
                    total = total
                        .checked_add(data.bytes.len())
                        .filter(|&n| n <= MAX_SELECTED_BYTES)
                        .ok_or_else(|| invalid("WNF query snapshot exceeds 16 MiB"))?;
                }
                Record::Status(status)
                    if !matches!(*status, STATUS_ACCESS_DENIED | STATUS_OBJECT_NAME_NOT_FOUND) =>
                {
                    return Err(invalid("WNF query recorded status is unmodeled"));
                }
                _ => {}
            }
            result.insert(definition.name, (definition, record));
        }
        Ok(Self {
            records: result,
            session,
        })
    }
    pub(crate) fn get(&self, name: u64) -> Option<&(Definition, Record)> {
        self.records.get(&name)
    }
}

/// Select at most six bounded native observations per definition. The caller
/// supplies a fixed metadata name, never a guest-selected name or pointer.
/// O(N log N + D) retained storage; D <= 16 MiB and N <= 4096.
pub(super) fn capture(
    definitions: Vec<Definition>,
    session: u32,
    mut query: impl FnMut(&Definition, &mut [u8]) -> (u32, u32, usize),
) -> io::Result<Queries> {
    let mut records = Vec::with_capacity(definitions.len().min(4096));
    if definitions.len() > 4096 {
        return Err(invalid("WNF query definition limit exceeded"));
    }
    let mut total = 0usize;
    for definition in definitions {
        let scope = Name::decode(definition.name)
            .ok_or_else(|| invalid("invalid WNF query definition"))?
            .scope;
        let mut record = Record::Unavailable("WNF state changed during bounded selection");
        if definition.maximum as usize > MAX_STATE_BYTES {
            record = Record::Unavailable("WNF state exceeds the 64 KiB selection limit");
        } else {
            let size = if scope == 3 {
                0
            } else {
                definition.maximum as usize
            };
            let observe = |query: &mut dyn FnMut(&Definition, &mut [u8]) -> (u32, u32, usize)| {
                let mut bytes = vec![0; size];
                let (status, stamp, length) = query(&definition, &mut bytes);
                if scope == 3 && matches!(status, STATUS_SUCCESS | STATUS_BUFFER_TOO_SMALL) {
                    // Only native read admission is selected. No host process-private
                    // data or change stamp becomes the new guest process's state.
                    return Record::Data(Data {
                        stamp: 0,
                        bytes: Arc::from([]),
                    });
                }
                if status == STATUS_SUCCESS && length <= bytes.len() {
                    bytes.truncate(length);
                    Record::Data(Data {
                        stamp,
                        bytes: bytes.into(),
                    })
                } else if matches!(status, STATUS_ACCESS_DENIED | STATUS_OBJECT_NAME_NOT_FOUND) {
                    Record::Status(status)
                } else {
                    Record::Unavailable("unmodeled native WNF state selection result")
                }
            };
            for _ in 0..3 {
                let first = observe(&mut query);
                let second = observe(&mut query);
                if first == second {
                    record = first;
                    break;
                }
            }
        }
        if let Record::Data(data) = &record {
            total = total
                .checked_add(data.bytes.len())
                .filter(|&n| n <= MAX_SELECTED_BYTES)
                .ok_or_else(|| invalid("WNF query capture exceeds 16 MiB"))?;
        }
        records.push((definition, record));
    }
    Queries::selected(records, session)
}
