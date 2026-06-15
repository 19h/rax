//! Regression guard (#174): `scripts/pgo-build.sh` must not default its PGO
//! profile directory to a predictable, world-writable shared path (e.g.
//! `/tmp/rax-pgo-data`). A fixed shared path lets a local attacker pre-create
//! or symlink it and poison the profile / clobber the build user's files. The
//! script must instead create a private, unpredictable directory with
//! `mktemp -d` and clean it up.

use std::fs;
use std::path::PathBuf;

#[test]
fn pgo_build_script_uses_private_temp_dir() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/pgo-build.sh");
    let src = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

    // Must NOT default the profile dir to a fixed shared /tmp path.
    assert!(
        !src.contains("/tmp/rax-pgo-data"),
        "pgo-build.sh must not default the PGO profile dir to a predictable \
         shared path (/tmp/rax-pgo-data); use `mktemp -d` instead"
    );

    // Must create a private, unpredictable directory for the default case.
    assert!(
        src.contains("mktemp -d"),
        "pgo-build.sh must create its default profile dir with `mktemp -d`"
    );

    // The merged profile must live inside the private dir, not a predictable
    // sibling like `$PROF.profdata`.
    assert!(
        !src.contains("$PROF.profdata"),
        "pgo-build.sh must keep the merged .profdata inside the private dir, \
         not at a predictable sibling path"
    );
}
