//! Immutable WNF provisioning and subject metadata selected with native DLLs.
//! Guest requests use private records and never acquire a host token or HKEY.
mod catalog;
mod query;
mod system;
pub(crate) use catalog::{Catalog, Name, Permission};
pub(crate) use query::{Data, Definition, Queries, Record};
#[cfg(windows)]
mod installed;
#[cfg(windows)]
mod installed_query;

#[cfg(windows)]
pub(crate) fn snapshot() -> std::io::Result<Catalog> {
    let values = super::registry::wnf_values()?;
    let selection = installed::subject()?;
    let definitions = values
        .iter()
        .map(Definition::parse)
        .collect::<std::io::Result<Vec<_>>>()?;
    let queries = installed_query::snapshot(definitions)?;
    if selection != installed::subject()? {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "native WNF subject or token access changed during query-state selection",
        ));
    }
    let (subject, access, session, scalars, bno) = selection;
    Ok(Catalog::selected(values, subject)?
        .with_primary_token_access(access)
        .with_primary_token_session_id(session)
        .with_primary_token_scalars(scalars)
        .with_primary_token_bno(bno)
        .with_queries(queries))
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

#[cfg(test)]
mod query_tests;

#[cfg(test)]
pub(crate) mod system_tests;
