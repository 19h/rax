//! One fixed per-user policy root, selected only from the captured primary SID.
use super::*;

pub(crate) fn user_policy_path(sid: &[u8]) -> io::Result<Vec<u16>> {
    if super::super::security::sid(sid) != Some(sid) {
        return Err(invalid("invalid primary SID for runtime policy selection"));
    }
    let authority = sid[2..8]
        .iter()
        .fold(0u64, |n, &byte| (n << 8) | u64::from(byte));
    let authority = if authority > u64::from(u32::MAX) {
        format!("0x{authority:X}")
    } else {
        authority.to_string()
    };
    let mut text = format!("S-1-{authority}");
    for subauthority in sid[8..].chunks_exact(4) {
        let value = u32::from_le_bytes(subauthority.try_into().unwrap());
        text.push_str(&format!("-{value}"));
    }
    Ok(format!(
        "\\Registry\\User\\{text}\\Software\\Policies\\Microsoft\\Windows\\Safer\\CodeIdentifiers"
    )
    .encode_utf16()
    .collect())
}

impl Registry {
    pub(crate) fn with_user_policy(
        mut self,
        sid: &[u8],
        tree: Option<KeyTree>,
    ) -> io::Result<Self> {
        if self.upcase.len() != 65_536 || self.user_policy.is_some() {
            return Err(invalid(
                "invalid or duplicate per-user runtime policy selection",
            ));
        }
        let path = user_policy_path(sid)?;
        let folded: Vec<_> = path.iter().map(|&u| self.upcase[usize::from(u)]).collect();
        if let Some(tree) = tree {
            let key = tree::build(
                self.upcase.clone(),
                path,
                tree,
                0,
                &mut self.value_budget()?,
                &mut tree::TreeBudget::default(),
            )?;
            self.keys.insert(folded.clone(), key);
        }
        self.user_policy = Some(folded.into());
        Ok(self)
    }
}
