//! AVX-512 specification inventory checks.
//!
//! This is a mnemonic-level inventory over the checked-in EVEX AVX-512 spec
//! pages. It does not prove semantic coverage for unimplemented instructions;
//! it makes the unimplemented set explicit and keeps it synchronized with the
//! local spec corpus.

#![cfg(feature = "x86_64-suite")]

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[path = "x86_64/avx512_inventory_data.rs"]
mod avx512_inventory_data;

use avx512_inventory_data::{
    RAX_EVEX_EXTRA_MNEMONICS_NOT_IN_AVX512_SPEC, RAX_EVEX_SIMD_DIFF_MNEMONICS,
};

const UNIMPLEMENTED_AVX512: &str = include_str!("x86_64/avx512_unimplemented_mnemonics.txt");

fn set_from_slice(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

fn set_from_manifest(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn assert_sorted_unique(name: &str, items: &[&str]) {
    for window in items.windows(2) {
        assert!(
            window[0] < window[1],
            "{name} must be sorted and unique: {:?}",
            window
        );
    }
}

fn assert_manifest_sorted_unique(name: &str, text: &str) {
    let items = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>();
    for window in items.windows(2) {
        assert!(
            window[0] < window[1],
            "{name} must be sorted and unique: {:?}",
            window
        );
    }
}

fn format_set(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join("\n")
}

fn extract_evex_mnemonic(cell: &str) -> Option<String> {
    let normalized = cell
        .replace("ibV", "ib V")
        .replace("iwV", "iw V")
        .replace("  ", " ");

    normalized.split_whitespace().find_map(|token| {
        let token = token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
        let mut chars = token.chars();
        let starts_like_vector_mnemonic = chars.next() == Some('V')
            && token.len() > 2
            && token.chars().all(|ch| ch.is_ascii_alphanumeric());
        if starts_like_vector_mnemonic && token != "VEX" {
            Some(token.to_ascii_lowercase())
        } else {
            None
        }
    })
}

fn avx512_spec_mnemonics() -> BTreeSet<String> {
    let spec_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/specifications/x86_64");
    let mut mnemonics = BTreeSet::new();

    for entry in fs::read_dir(&spec_dir).expect("x86_64 spec directory must exist") {
        let entry = entry.expect("spec directory entry must be readable");
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("txt") {
            continue;
        }

        let text = fs::read_to_string(entry.path()).expect("x86_64 spec file must be readable");
        for line in text.lines() {
            if !line.contains('|') || !line.contains("EVEX") || !line.contains("AVX512") {
                continue;
            }

            for cell in line.split('|').map(str::trim) {
                if !cell.starts_with("EVEX") {
                    continue;
                }
                if cell.contains("Instruction") && cell.contains("CPUID") {
                    continue;
                }
                if let Some(mnemonic) = extract_evex_mnemonic(cell) {
                    mnemonics.insert(mnemonic);
                }
            }
        }
    }

    mnemonics
}

#[test]
fn avx512_spec_inventory_is_partitioned() {
    assert_sorted_unique("RAX_EVEX_SIMD_DIFF_MNEMONICS", RAX_EVEX_SIMD_DIFF_MNEMONICS);
    assert_sorted_unique(
        "RAX_EVEX_EXTRA_MNEMONICS_NOT_IN_AVX512_SPEC",
        RAX_EVEX_EXTRA_MNEMONICS_NOT_IN_AVX512_SPEC,
    );
    assert_manifest_sorted_unique("avx512_unimplemented_mnemonics.txt", UNIMPLEMENTED_AVX512);

    let spec = avx512_spec_mnemonics();
    let implemented = set_from_slice(RAX_EVEX_SIMD_DIFF_MNEMONICS);
    let implemented_extra = set_from_slice(RAX_EVEX_EXTRA_MNEMONICS_NOT_IN_AVX512_SPEC);
    let known_unimplemented = set_from_manifest(UNIMPLEMENTED_AVX512);

    let actual_extra = implemented
        .difference(&spec)
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual_extra, implemented_extra,
        "rax-supported EVEX mnemonics outside the AVX-512 spec corpus changed"
    );

    let overlap = implemented
        .intersection(&known_unimplemented)
        .cloned()
        .collect::<BTreeSet<_>>();
    assert!(
        overlap.is_empty(),
        "mnemonics cannot be both implemented and known-unimplemented:\n{}",
        format_set(&overlap)
    );

    let unimplemented_not_in_spec = known_unimplemented
        .difference(&spec)
        .cloned()
        .collect::<BTreeSet<_>>();
    assert!(
        unimplemented_not_in_spec.is_empty(),
        "known-unimplemented manifest contains mnemonics not in the AVX-512 spec corpus:\n{}",
        format_set(&unimplemented_not_in_spec)
    );

    let mut partition = known_unimplemented.clone();
    partition.extend(implemented.intersection(&spec).cloned());

    let missing = spec
        .difference(&partition)
        .cloned()
        .collect::<BTreeSet<_>>();
    let unexpected = partition
        .difference(&spec)
        .cloned()
        .collect::<BTreeSet<_>>();

    assert!(
        missing.is_empty() && unexpected.is_empty(),
        "AVX-512 spec inventory mismatch\nmissing from implemented-or-unimplemented partition:\n{}\nunexpected in partition:\n{}",
        format_set(&missing),
        format_set(&unexpected)
    );
}
