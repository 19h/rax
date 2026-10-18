//! Fixed native token-query properties captured before guest execution.
use std::collections::BTreeMap;

pub(crate) const CLASSES: [u32; 15] = [15, 18, 20, 21, 23, 24, 26, 27, 32, 40, 42, 45, 46, 47, 48];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapturedScalar {
    Value(u32),
    InvalidClass,
}

/// Missing capture and a measured unavailable native class are distinct.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Scalars(BTreeMap<u32, CapturedScalar>);

pub(crate) fn is_class(class: u32) -> bool {
    CLASSES.contains(&class)
}

pub(crate) fn width(class: u32) -> Option<u32> {
    match class {
        21 => Some(1),
        45 | 46 => None,
        _ if is_class(class) => Some(4),
        _ => None,
    }
}

impl Scalars {
    /// At most 15 entries; rejects unknown classes, duplicates and values
    /// outside the supported wire width. No host handle or address is stored.
    pub(crate) fn selected(
        values: impl IntoIterator<Item = (u32, CapturedScalar)>,
    ) -> Option<Self> {
        let mut result = BTreeMap::new();
        for (class, value) in values {
            if !is_class(class) || result.contains_key(&class) {
                return None;
            }
            if let CapturedScalar::Value(value) = value {
                let valid = match class {
                    21 => value <= u32::from(u8::MAX),
                    45 | 46 => false,
                    _ => true,
                };
                if !valid {
                    return None;
                }
            }
            result.insert(class, value);
        }
        Some(Self(result))
    }

    pub(crate) fn get(&self, class: u32) -> Option<CapturedScalar> {
        self.0.get(&class).copied()
    }
}

#[cfg(test)]
mod tests;
