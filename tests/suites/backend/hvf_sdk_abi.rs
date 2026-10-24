//! Source ABI parity; this test does not create a VM or execute a guest.
#![cfg(target_os = "macos")]

use std::path::Path;
use std::process::Command;

#[test]
fn handwritten_intel_bindings_match_installed_apple_sdk() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("hvf-sdk-abi-{}", std::process::id()));
    let output = Command::new("python3")
        .arg(root.join("tools/hvf/check_sdk_abi.py"))
        .arg("--repo")
        .arg(root)
        .arg("--work-dir")
        .arg(&work)
        .output()
        .expect("Apple SDK ABI probe requires python3, stable Rust and xcrun clang");
    assert!(
        output.status.success(),
        "SDK ABI mismatch; evidence: {}\nstdout: {}\nstderr: {}",
        work.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
