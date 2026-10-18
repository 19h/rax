//! Owned primary-token base named-object isolation metadata.

/// The largest captured query is 64 KiB, including the widest native header
/// and one UTF-16 terminator: (65536 - 16) / 2 - 1 = 32759 code units.
const MAX_PREFIX_UNITS: usize = 32759;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Isolation {
    prefix: Option<Vec<u16>>,
}

impl Isolation {
    pub(crate) fn new(prefix: Option<Vec<u16>>) -> Option<Self> {
        if prefix
            .as_ref()
            .is_some_and(|text| text.len() > MAX_PREFIX_UNITS || text.contains(&0))
        {
            return None;
        }
        Some(Self { prefix })
    }

    pub(crate) fn prefix(&self) -> Option<&[u16]> {
        self.prefix.as_deref()
    }

    /// O(1), with a 65536-byte maximum. Native header padding is not payload.
    pub(crate) fn required_bytes(&self, pointer_bytes: usize) -> Option<u32> {
        if !matches!(pointer_bytes, 4 | 8) {
            return None;
        }
        let tail = self.prefix.as_ref().map_or(0, |text| 2 * (text.len() + 1));
        Some((2 * pointer_bytes + tail) as u32)
    }

    /// O(N) copy of at most 32759 UTF-16 units. A host address is validated
    /// against the owned buffer and never retained in the model.
    #[cfg(any(windows, test))]
    pub(crate) fn from_native(bytes: &[u8], base: u64, width: usize) -> Option<Self> {
        if !matches!(width, 4 | 8) || bytes.len() < 2 * width || bytes.len() > 65536 {
            return None;
        }
        let mut word = [0u8; 8];
        word[..width].copy_from_slice(&bytes[..width]);
        let pointer = u64::from_le_bytes(word);
        match bytes[width] {
            0 if pointer == 0 && bytes.len() == 2 * width => Self::new(None),
            1 if pointer == base.checked_add((2 * width) as u64)? => {
                let tail = &bytes[2 * width..];
                if tail.len() < 2 || tail.len() % 2 != 0 || tail[tail.len() - 2..] != [0, 0] {
                    return None;
                }
                let text = tail[..tail.len() - 2]
                    .chunks_exact(2)
                    .map(|unit| u16::from_le_bytes(unit.try_into().unwrap()))
                    .collect();
                Self::new(Some(text))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
