//! Source-level inventory for x86_64 instruction paths that still report
//! unimplemented, unhandled, or unsupported diagnostics.

#![cfg(feature = "x86_64-suite")]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const INVENTORY: &str =
    include_str!("../../../generated/x86_64/inventories/unimplemented_source_sites.txt");
const SMIR_LIFT_ROOT: &str = "src/smir/lift/x86_64";
const SMIR_LIFT_TEST_ROOT: &str = "src/smir/lift/x86_64/tests/";
const SOURCE_ROOTS: &[&str] = &["src/isa/x86_64", SMIR_LIFT_ROOT];
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
    "mixed-dispatch",
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
    let mut files = Vec::new();
    for source in SOURCE_ROOTS {
        let source = root.join(source);
        if source.is_dir() {
            rust_files(&source, &mut files);
        } else {
            files.push(source);
        }
    }
    files.sort();

    let mut diagnostics = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if relative.starts_with(SMIR_LIFT_TEST_ROOT) {
            continue;
        }

        let text = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", file.display()));
        let is_smir_lift_source = relative.starts_with(SMIR_LIFT_ROOT);
        for (line_index, line) in text.lines().enumerate() {
            let live = if is_smir_lift_source {
                line.contains("LiftError::Unsupported")
            } else {
                line_has_live_diagnostic(line)
            };
            if live {
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
            SOURCE_ROOTS.iter().any(|root| entry.path.starts_with(root)),
            "{} must live under an inventoried x86 source root",
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
}

#[test]
fn x86_64_vector_modrm_prefixes_use_canonical_address_projection() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join(SMIR_LIFT_ROOT), &mut files);
    files.sort();

    let mut violations = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let vector_dispatch = relative.starts_with("src/smir/lift/x86_64/dispatch/vector");
        let vector_semantics = relative.starts_with("src/smir/lift/x86_64/simd/")
            && relative != "src/smir/lift/x86_64/simd/xop.rs";
        if !vector_dispatch && !vector_semantics {
            continue;
        }

        let text = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", file.display()));
        for (line, source) in text.lines().enumerate() {
            if source.contains("X86Prefix::default()") {
                violations.push(format!("{relative}:{}: {}", line + 1, source.trim()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "vector ModR/M decoding must project VecPrefix through modrm_prefix() so 67H, FS/GS, \
         EVEX.B4, and EVEX.X4 cannot be dropped:\n{}",
        violations.join("\n")
    );
}
