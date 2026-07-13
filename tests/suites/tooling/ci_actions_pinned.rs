use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn external_github_actions_are_pinned_to_full_commit_shas() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();

    for dir in [".github/actions", ".github/workflows"] {
        collect_yaml_files(&root.join(dir), &mut files);
    }

    let mut violations = Vec::new();
    for file in files {
        let contents = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
        let rel = file.strip_prefix(&root).unwrap_or(&file);

        for (line_idx, line) in contents.lines().enumerate() {
            let Some(action_ref) = parse_uses_value(line) else {
                continue;
            };

            if is_local_action(action_ref) || action_ref.starts_with("docker://") {
                continue;
            }

            if !is_full_sha_ref(action_ref) {
                violations.push(format!(
                    "{}:{} uses {}",
                    rel.display(),
                    line_idx + 1,
                    action_ref
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "external GitHub Actions must be pinned to full commit SHAs:\n{}",
        violations.join("\n")
    );
}

#[test]
fn scheduled_differential_separates_oracles_diagnostics_and_assembler_capabilities() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workflow = root.join(".github/workflows/differential.yml");
    let contents = fs::read_to_string(&workflow)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", workflow.display()));
    let normalized = contents.split_whitespace().collect::<Vec<_>>().join(" ");

    for required in [
        "--include-ignored",
        "--skip report_evex_spec_forms_rejected_by_smir_lifter",
        "--skip qemu_evex_unimplemented_avx512_corpus_matches_rax_when_enabled",
        "evex_generated_corpus_covers_supported_selectors_and_forms \\ -- --exact --nocapture",
        "skips=--skip evex_generated_corpus_covers_supported_selectors_and_forms --skip qemu_evex_generated_corpus_matches_rax",
    ] {
        assert!(
            normalized.contains(required),
            "scheduled differential workflow is missing required policy fragment: {required}"
        );
    }
}

#[test]
fn scheduled_kvm_keeps_host_dependent_differentials_informational() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workflow = root.join(".github/workflows/kvm.yml");
    let contents = fs::read_to_string(&workflow)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", workflow.display()));

    for job in ["kvm-differential", "kvm-avx512"] {
        let marker = format!("  {job}:\n");
        let start = contents
            .find(&marker)
            .unwrap_or_else(|| panic!("KVM workflow is missing {job}"));
        let section = contents[start + marker.len()..]
            .lines()
            .take_while(|line| line.trim().is_empty() || line.starts_with("    "))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            section.contains("continue-on-error: true"),
            "host-dependent {job} must retain diagnostics without failing the workflow"
        );
    }
}

#[test]
fn push_ci_runs_host_specific_scalar_and_evex_jit_regressions() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workflow = root.join(".github/workflows/ci.yml");
    let contents = fs::read_to_string(&workflow)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", workflow.display()));

    for target in [
        "--test smir_jit_vcpu",
        "--test smir_jit_x86_aarch64",
        "--test smir_jit_evex_masking",
    ] {
        assert!(
            contents.contains(target),
            "push CI core slice must execute {target}"
        );
    }
}

fn collect_yaml_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|err| panic!("failed to read {}: {err}", dir.display()));

    for entry in entries {
        let path = entry
            .unwrap_or_else(|err| panic!("failed to read entry in {}: {err}", dir.display()))
            .path();
        if path.is_dir() {
            collect_yaml_files(&path, files);
        } else if is_yaml_file(&path) {
            files.push(path);
        }
    }
}

fn is_yaml_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("yml" | "yaml")
    )
}

fn parse_uses_value(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let value = trimmed
        .strip_prefix("- uses:")
        .or_else(|| trimmed.strip_prefix("uses:"))?
        .split('#')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(|ch| ch == '"' || ch == '\'');

    if value.is_empty() { None } else { Some(value) }
}

fn is_local_action(action_ref: &str) -> bool {
    action_ref.starts_with("./") || action_ref.starts_with("../")
}

fn is_full_sha_ref(action_ref: &str) -> bool {
    let Some((_, reference)) = action_ref.rsplit_once('@') else {
        return false;
    };

    reference.len() == 40 && reference.bytes().all(|byte| byte.is_ascii_hexdigit())
}
