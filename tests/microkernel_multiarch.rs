//! End-to-end multi-architecture microkernel test.
//!
//! Boots the bare-metal microkernel (`microkernel/microkernel-<arch>.bin`) under
//! the rax emulator for x86_64, AArch64 and ARMv6, and asserts that each one:
//!   * prints the `RAX-MK: RESULT PASS` sentinel (every in-guest check passed),
//!   * does not print `RAX-MK: RESULT FAIL`, and
//!   * reports the same `NBODY_CKSUM` as the others (cross-arch determinism).
//!
//! The kernel binaries are produced by `microkernel/build.sh` (nightly +
//! build-std + a custom ARMv6 target). If they are absent the test is skipped,
//! so it stays green on hosts without that toolchain; the dedicated
//! `microkernel` CI workflow builds them first and therefore enforces PASS.
//!
//! `rax` itself is located via `CARGO_BIN_EXE_rax`, so `cargo test` builds it
//! with whatever feature set the test run uses (the emulator backend is always
//! present). Run with, e.g.:
//!
//!   cargo test --no-default-features --test microkernel_multiarch -- --nocapture

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

struct Case {
    label: &'static str,
    arch: &'static str,
    extra: Vec<String>,
}

struct BootResult {
    output: String,
    status: ExitStatus,
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn cases() -> Vec<Case> {
    let dtb = manifest_dir().join("microkernel/dtb/s3c6410-smdk6410.dtb");
    vec![
        Case {
            label: "x86_64",
            arch: "x86-64",
            extra: vec![],
        },
        Case {
            label: "aarch64",
            arch: "aarch64",
            extra: vec![],
        },
        Case {
            label: "armv6",
            arch: "armv7a",
            extra: vec!["--dtb".into(), dtb.to_string_lossy().into_owned()],
        },
    ]
}

fn kernel_path(label: &str) -> PathBuf {
    manifest_dir().join(format!("microkernel/microkernel-{label}.bin"))
}

/// Run one kernel under rax. Returns the combined stdout+stderr and exit status.
fn boot(case: &Case, bin: &Path) -> BootResult {
    let rax = env!("CARGO_BIN_EXE_rax");
    let mut cmd = Command::new(rax);
    cmd.args([
        "--backend",
        "emulator",
        "--arch",
        case.arch,
        "--memory",
        "128M",
        "--kernel",
        bin.to_str().unwrap(),
    ]);
    cmd.args(&case.extra);
    // The kernel always powers the machine off (ACPI / PSCI / S3C poweroff), so
    // the process terminates on its own; `output()` drains both pipes and waits.
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn rax for {}: {e}", case.label));
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    BootResult {
        output: combined,
        status: out.status,
    }
}

fn extract_cksum(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|l| l.trim().strip_prefix("NBODY_CKSUM=").map(|s| s.to_string()))
}

fn output_tail(output: &str) -> String {
    output
        .lines()
        .rev()
        .take(25)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

fn boot_failure(
    label: &str,
    output: &str,
    status_success: bool,
    status: impl std::fmt::Display,
) -> Option<String> {
    let tail = output_tail(output);
    if !status_success {
        return Some(format!(
            "[{label}] emulator exited with {status}. Tail of output:\n{tail}"
        ));
    }

    let passed = output.contains("RAX-MK: RESULT PASS");
    let failed = output.contains("RAX-MK: RESULT FAIL");
    if !(passed && !failed) {
        return Some(format!("[{label}] did not pass. Tail of output:\n{tail}"));
    }

    None
}

#[test]
fn microkernel_passes_on_all_architectures() {
    let cases = cases();

    // Skip cleanly if the bare-metal binaries have not been built.
    let missing: Vec<_> = cases
        .iter()
        .filter(|c| !kernel_path(c.label).exists())
        .map(|c| c.label)
        .collect();
    if !missing.is_empty() {
        let msg = format!(
            "microkernel binaries not built ({}). Run `microkernel/build.sh all` first.",
            missing.join(", ")
        );
        // CI sets MICROKERNEL_REQUIRE=1 so a missing binary is a hard failure
        // rather than a silent skip; locally we skip to stay green without the
        // bare-metal toolchain.
        if std::env::var_os("MICROKERNEL_REQUIRE").is_some() {
            panic!("{msg}");
        }
        eprintln!("SKIP: {msg}");
        return;
    }

    let mut checksums: Vec<(&str, String)> = Vec::new();
    for case in &cases {
        let bin = kernel_path(case.label);
        let result = boot(case, &bin);
        if let Some(message) = boot_failure(
            case.label,
            &result.output,
            result.status.success(),
            &result.status,
        ) {
            panic!("{message}");
        }

        let cksum = extract_cksum(&result.output)
            .unwrap_or_else(|| panic!("[{}] no NBODY_CKSUM in output", case.label));
        eprintln!("[{}] RESULT PASS, NBODY_CKSUM={cksum}", case.label);
        checksums.push((case.label, cksum));
    }

    // Cross-architecture determinism: the pure-integer n-body checksum must be
    // bit-identical on x86_64, AArch64 and ARMv6.
    let first = &checksums[0].1;
    for (label, c) in &checksums[1..] {
        assert_eq!(
            c, first,
            "NBODY_CKSUM mismatch: {} reported {c}, x86_64 reported {first}",
            label
        );
    }
}

#[test]
fn rejects_pass_sentinel_with_nonzero_emulator_exit() {
    let output = "RAX-MK: RESULT PASS\nNBODY_CKSUM=0x1\n";
    let err = boot_failure("fake", output, false, "exit status: 1")
        .expect("non-zero status must reject a PASS sentinel");

    assert!(err.contains("[fake] emulator exited with exit status: 1"));
    assert!(err.contains("RAX-MK: RESULT PASS"));
}
