//! Formatting invariants for checked-in x86_64 unimplemented-instruction manifests.

#![cfg(feature = "x86_64-suite")]

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[path = "x86_64/avx512_inventory_data.rs"]
mod avx512_inventory_data;

use avx512_inventory_data::RAX_EVEX_SIMD_DIFF_MNEMONICS;

const INTEL_INTRINSICS_XML: &str =
    include_str!("../docs/specifications/x86_64/intel-intrinsics-guide-3-6-9.xml");

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

const AVX10_CPUIDS: &[&str] = &[
    "AVX_IFMA",
    "AVX_NE_CONVERT",
    "AVX_VNNI",
    "AVX_VNNI_INT8",
    "AVX_VNNI_INT16",
];

const RAX_AVX512_VEX_OPMASK_MNEMONICS: &[&str] = &[
    "kaddb", "kaddd", "kaddq", "kaddw", "kandb", "kandd", "kandnb", "kandnd", "kandnq", "kandnw",
    "kandq", "kandw", "kmovb", "kmovd", "kmovq", "kmovw", "knotb", "knotd", "knotq", "knotw",
    "korb", "kord", "korq", "korw", "kxnorb", "kxnord", "kxnorq", "kxnorw", "kxorb", "kxord",
    "kxorq", "kxorw",
];

#[derive(Clone, Copy)]
enum ExtensionBucket {
    Avx,
    Avx2,
    Avx10,
    Avx512,
    Apx,
}

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
            entry
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_'),
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

fn manifest_set(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn tag_values(block: &str, tag: &str) -> BTreeSet<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut values = BTreeSet::new();
    let mut rest = block;
    while let Some(start) = rest.find(&open) {
        rest = &rest[start + open.len()..];
        let Some(end) = rest.find(&close) else {
            break;
        };
        values.insert(rest[..end].trim().to_string());
        rest = &rest[end + close.len()..];
    }
    values
}

fn instruction_names(block: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut rest = block;
    while let Some(start) = rest.find("<instruction") {
        rest = &rest[start..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..end];
        if let Some(name) = attr_value(tag, "name") {
            names.insert(name.to_ascii_lowercase());
        }
        rest = &rest[end..];
    }
    names
}

fn intrinsics_xml_mnemonics(bucket: ExtensionBucket) -> BTreeSet<String> {
    let mut mnemonics = BTreeSet::new();
    for chunk in INTEL_INTRINSICS_XML.split("<intrinsic ").skip(1) {
        let Some(end) = chunk.find("</intrinsic>") else {
            continue;
        };
        let block = &chunk[..end];
        let tag_end = block.find('>').unwrap_or(block.len());
        let tag = &block[..tag_end];
        let tech = attr_value(tag, "tech").unwrap_or_default();
        let cpuid = tag_values(block, "CPUID");

        let matches_bucket = match bucket {
            ExtensionBucket::Avx => tech == "AVX_ALL" && cpuid.contains("AVX"),
            ExtensionBucket::Avx2 => tech == "AVX_ALL" && cpuid.contains("AVX2"),
            ExtensionBucket::Avx10 => {
                tech == "AVX_ALL" && AVX10_CPUIDS.iter().any(|feature| cpuid.contains(*feature))
            }
            ExtensionBucket::Avx512 => {
                tech != "SVML"
                    && (tech == "AVX-512"
                        || cpuid.iter().any(|feature| feature.starts_with("AVX512")))
            }
            ExtensionBucket::Apx => {
                tech == "APX" || cpuid.iter().any(|feature| feature.contains("APX"))
            }
        };
        if !matches_bucket {
            continue;
        }

        for mnemonic in instruction_names(block) {
            let is_extension_mnemonic = match bucket {
                ExtensionBucket::Avx | ExtensionBucket::Avx2 | ExtensionBucket::Avx10 => {
                    mnemonic.starts_with('v')
                }
                ExtensionBucket::Avx512 => mnemonic.starts_with('v') || mnemonic.starts_with('k'),
                ExtensionBucket::Apx => true,
            };
            if is_extension_mnemonic {
                mnemonics.insert(mnemonic);
            }
        }
    }
    mnemonics
}

fn local_simd_test_mnemonics(
    bucket: ExtensionBucket,
    candidates: &BTreeSet<String>,
) -> BTreeSet<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/x86_64/simd");
    let mut covered = BTreeSet::new();
    let subdirs: &[&str] = match bucket {
        ExtensionBucket::Avx => &["avx"],
        ExtensionBucket::Avx2 => &["avx2"],
        ExtensionBucket::Avx10 => &["avx10"],
        ExtensionBucket::Avx512 => &["avx512"],
        ExtensionBucket::Apx => &[],
    };
    for subdir in subdirs {
        let dir = root.join(subdir);
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            if path.file_stem().and_then(|stem| stem.to_str()) == Some("mod") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            let text = fs::read_to_string(&path)
                .unwrap_or_default()
                .to_ascii_lowercase();
            for candidate in candidates {
                if stem.split('_').any(|part| part == candidate)
                    || text.contains(&format!("test_{candidate}"))
                {
                    covered.insert(candidate.clone());
                }
            }
        }
    }
    covered
}

fn expected_unimplemented_mnemonics(bucket: ExtensionBucket) -> BTreeSet<String> {
    let xml = intrinsics_xml_mnemonics(bucket);
    let mut implemented = BTreeSet::new();

    if !matches!(bucket, ExtensionBucket::Avx512) {
        implemented.extend(local_simd_test_mnemonics(bucket, &xml));
    }
    implemented.extend(
        RAX_EVEX_SIMD_DIFF_MNEMONICS
            .iter()
            .map(|mnemonic| (*mnemonic).to_string()),
    );
    if matches!(bucket, ExtensionBucket::Avx512) {
        implemented.extend(
            RAX_AVX512_VEX_OPMASK_MNEMONICS
                .iter()
                .map(|mnemonic| (*mnemonic).to_string()),
        );
    }

    xml.difference(&implemented).cloned().collect()
}

fn assert_manifest_matches_intel_intrinsics_xml(name: &str, text: &str, bucket: ExtensionBucket) {
    let actual = manifest_set(text);
    let expected = expected_unimplemented_mnemonics(bucket);
    assert_eq!(
        actual, expected,
        "{name} must match the unimplemented mnemonic set derived from intel-intrinsics-guide-3-6-9.xml"
    );
}

#[test]
fn x86_64_unimplemented_mnemonic_manifests_match_intel_intrinsics_xml() {
    assert_manifest_matches_intel_intrinsics_xml(
        "avx_unimplemented_mnemonics.txt",
        include_str!("x86_64/avx_unimplemented_mnemonics.txt"),
        ExtensionBucket::Avx,
    );
    assert_manifest_matches_intel_intrinsics_xml(
        "avx2_unimplemented_mnemonics.txt",
        include_str!("x86_64/avx2_unimplemented_mnemonics.txt"),
        ExtensionBucket::Avx2,
    );
    assert_manifest_matches_intel_intrinsics_xml(
        "avx10_unimplemented_mnemonics.txt",
        include_str!("x86_64/avx10_unimplemented_mnemonics.txt"),
        ExtensionBucket::Avx10,
    );
    assert_manifest_matches_intel_intrinsics_xml(
        "avx512_unimplemented_mnemonics.txt",
        include_str!("x86_64/avx512_unimplemented_mnemonics.txt"),
        ExtensionBucket::Avx512,
    );
    assert_manifest_matches_intel_intrinsics_xml(
        "apx_unimplemented_mnemonics.txt",
        include_str!("x86_64/apx_unimplemented_mnemonics.txt"),
        ExtensionBucket::Apx,
    );
}
