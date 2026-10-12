//! Immutable WNF provisioning and subject metadata selected with native DLLs.
//! Guest requests use private records and never acquire a host token or HKEY.
mod catalog;
pub(crate) use catalog::{Catalog, Name, Permission};
#[cfg(windows)]
mod installed;

#[cfg(windows)]
pub(crate) fn snapshot() -> std::io::Result<Catalog> {
    Catalog::selected(super::registry::wnf_values()?, installed::subject()?)
}

#[cfg(not(windows))]
pub(crate) fn snapshot() -> std::io::Result<Catalog> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "installed WNF metadata requires Windows",
    ))
}

#[cfg(test)]
pub(crate) mod tests;
