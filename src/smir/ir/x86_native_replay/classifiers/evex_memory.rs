//! Shared byte-structural helpers for exact EVEX memory classifiers.

pub(super) fn memory_operand_end(bytes: &[u8], modrm_index: usize) -> Option<usize> {
    let modrm = *bytes.get(modrm_index)?;
    let mode = modrm >> 6;
    let rm = modrm & 7;
    if mode == 3 {
        return None;
    }

    let mut end = modrm_index + 1;
    let sib_base = if rm == 4 {
        let sib = *bytes.get(end)?;
        end += 1;
        Some(sib & 7)
    } else {
        None
    };
    let displacement = match mode {
        0 if rm == 5 || sib_base == Some(5) => 4,
        0 => 0,
        1 => 1,
        2 => 4,
        _ => unreachable!("register mode rejected"),
    };
    end.checked_add(displacement)
        .filter(|operand_end| *operand_end <= bytes.len())
}

pub(super) fn vector_legacy_prefix_len(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .take_while(|byte| matches!(byte, 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x67))
        .count()
}
