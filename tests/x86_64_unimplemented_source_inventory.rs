//! Source-level inventory for x86_64 instruction paths that still report
//! unimplemented, unhandled, or unsupported diagnostics.

#![cfg(feature = "x86_64-suite")]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const INVENTORY: &str = include_str!("x86_64/unimplemented_source_sites.txt");
const SOURCE_ROOT: &str = "src/backend/emulator/x86_64";
const DIAGNOSTIC_WORDS: &[&str] = &[
    "unimplemented",
    "not implemented",
    "unsupported",
    "unhandled",
];
const CLASSIFICATIONS: &[&str] = &[
    "dead-diagnostic",
    "encoding-hole",
    "manifest-diff",
    "non-instruction",
    "system-gap",
    "valid-gap-needs-diff",
];

#[derive(Clone, Debug, Eq, PartialEq)]
struct InventoryEntry {
    path: String,
    count: usize,
    classification: String,
    needle: String,
}

#[derive(Clone, Debug)]
struct SourceDiagnostic {
    path: String,
    line: usize,
    text: String,
}

fn parse_inventory() -> Vec<InventoryEntry> {
    INVENTORY
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }

            let parts = line.splitn(4, '|').collect::<Vec<_>>();
            assert_eq!(
                parts.len(),
                4,
                "unimplemented_source_sites.txt:{} must have 4 pipe-separated fields",
                index + 1
            );
            let count = parts[1].parse::<usize>().unwrap_or_else(|error| {
                panic!(
                    "unimplemented_source_sites.txt:{} has invalid count: {error}",
                    index + 1
                )
            });
            assert!(
                count > 0,
                "unimplemented_source_sites.txt:{} count must be non-zero",
                index + 1
            );
            assert!(
                CLASSIFICATIONS.contains(&parts[2]),
                "unimplemented_source_sites.txt:{} has unknown classification {:?}",
                index + 1,
                parts[2]
            );
            Some(InventoryEntry {
                path: parts[0].to_string(),
                count,
                classification: parts[2].to_string(),
                needle: parts[3].to_string(),
            })
        })
        .collect()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", dir.display());
    }) {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn line_has_live_diagnostic(line: &str) -> bool {
    let trimmed = line.trim_start();
    !trimmed.starts_with("//")
        && trimmed.contains('"')
        && DIAGNOSTIC_WORDS.iter().any(|word| trimmed.contains(word))
}

fn source_diagnostics() -> Vec<SourceDiagnostic> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_root = root.join(SOURCE_ROOT);
    let mut files = Vec::new();
    rust_files(&source_root, &mut files);

    let mut diagnostics = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", file.display()));
        let relative = file
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        for (line_index, line) in text.lines().enumerate() {
            if line_has_live_diagnostic(line) {
                diagnostics.push(SourceDiagnostic {
                    path: relative.clone(),
                    line: line_index + 1,
                    text: line.trim().to_string(),
                });
            }
        }
    }

    diagnostics
}

fn assert_inventory_sorted_unique(entries: &[InventoryEntry]) {
    let mut previous: Option<(&str, &str)> = None;
    for entry in entries {
        assert!(
            entry.path.starts_with(SOURCE_ROOT),
            "{} must live under {SOURCE_ROOT}",
            entry.path
        );
        let key = (entry.path.as_str(), entry.needle.as_str());
        if let Some(previous) = previous {
            assert!(
                previous < key,
                "unimplemented_source_sites.txt must be sorted and unique: {:?} before {:?}",
                previous,
                key
            );
        }
        previous = Some(key);
    }
}

fn format_uncovered(diagnostics: Vec<SourceDiagnostic>) -> String {
    diagnostics
        .into_iter()
        .map(|diagnostic| {
            format!(
                "{}:{}: {}",
                diagnostic.path, diagnostic.line, diagnostic.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn x86_64_unimplemented_source_diagnostics_are_inventoried() {
    let entries = parse_inventory();
    assert_inventory_sorted_unique(&entries);

    let diagnostics = source_diagnostics();
    let mut unmatched = Vec::new();
    for diagnostic in &diagnostics {
        let covered = entries
            .iter()
            .any(|entry| diagnostic.path == entry.path && diagnostic.text.contains(&entry.needle));
        if !covered {
            unmatched.push(diagnostic.clone());
        }
    }
    assert!(
        unmatched.is_empty(),
        "x86_64 unimplemented source diagnostics missing from inventory:\n{}",
        format_uncovered(unmatched)
    );

    let mut duplicate_entries = BTreeSet::new();
    let mut seen_entries = BTreeSet::new();
    for entry in &entries {
        let key = (entry.path.as_str(), entry.needle.as_str());
        if !seen_entries.insert(key) {
            duplicate_entries.insert(format!("{}|{}", entry.path, entry.needle));
        }
    }
    assert!(
        duplicate_entries.is_empty(),
        "duplicate source inventory entries:\n{}",
        duplicate_entries.into_iter().collect::<Vec<_>>().join("\n")
    );

    let mut count_failures = Vec::new();
    for entry in &entries {
        let actual = diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.path == entry.path && diagnostic.text.contains(&entry.needle)
            })
            .count();
        if actual != entry.count {
            count_failures.push(format!(
                "{}: expected {} occurrence(s) of {:?}, found {}",
                entry.path, entry.count, entry.needle, actual
            ));
        }
    }
    assert!(
        count_failures.is_empty(),
        "x86_64 unimplemented source inventory occurrence mismatch:\n{}",
        count_failures.join("\n")
    );

    let mut by_classification = BTreeMap::<&str, usize>::new();
    for entry in &entries {
        *by_classification
            .entry(entry.classification.as_str())
            .or_default() += entry.count;
    }
    assert!(
        by_classification.contains_key("valid-gap-needs-diff"),
        "source inventory must keep valid unimplemented gaps visible until they are covered"
    );
}
