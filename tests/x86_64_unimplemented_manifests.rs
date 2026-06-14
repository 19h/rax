//! Formatting invariants for checked-in x86_64 unimplemented-instruction manifests.

#![cfg(feature = "x86_64-suite")]

const MANIFESTS: &[(&str, &str)] = &[
    (
        "avx_unimplemented_mnemonics.txt",
        include_str!("x86_64/avx_unimplemented_mnemonics.txt"),
    ),
    (
        "avx2_unimplemented_mnemonics.txt",
        include_str!("x86_64/avx2_unimplemented_mnemonics.txt"),
    ),
    (
        "avx10_unimplemented_mnemonics.txt",
        include_str!("x86_64/avx10_unimplemented_mnemonics.txt"),
    ),
    (
        "avx512_unimplemented_mnemonics.txt",
        include_str!("x86_64/avx512_unimplemented_mnemonics.txt"),
    ),
    (
        "apx_unimplemented_mnemonics.txt",
        include_str!("x86_64/apx_unimplemented_mnemonics.txt"),
    ),
];

fn manifest_entries<'a>(name: &str, text: &'a str) -> Vec<(usize, &'a str)> {
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                None
            } else {
                assert_eq!(
                    line,
                    trimmed,
                    "{name}:{} entry has leading or trailing whitespace",
                    index + 1
                );
                Some((index + 1, trimmed))
            }
        })
        .collect()
}

fn assert_manifest_entries_well_formed(name: &str, entries: &[(usize, &str)]) {
    for (line, entry) in entries {
        assert_eq!(
            *entry,
            entry.to_ascii_lowercase(),
            "{name}:{line} entry must be lowercase"
        );
        assert!(
            entry.chars().all(|ch| ch.is_ascii_lowercase()
                || ch.is_ascii_digit()
                || ch == '_'),
            "{name}:{line} entry must contain only lowercase ASCII letters, digits, or underscores: {entry}"
        );
    }
}

fn assert_manifest_entries_sorted_unique(name: &str, entries: &[(usize, &str)]) {
    for window in entries.windows(2) {
        let (prev_line, prev) = window[0];
        let (line, entry) = window[1];
        assert!(
            prev < entry,
            "{name}:{line} entry must be sorted and unique: {prev:?} at line {prev_line}, {entry:?} at line {line}"
        );
    }
}

#[test]
fn x86_64_unimplemented_mnemonic_manifests_are_sorted_unique() {
    for (name, text) in MANIFESTS {
        let entries = manifest_entries(name, text);
        assert_manifest_entries_well_formed(name, &entries);
        assert_manifest_entries_sorted_unique(name, &entries);
    }
}
