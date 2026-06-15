use std::fs;
use std::path::PathBuf;

#[test]
fn pgo_build_uses_private_temporary_profile_paths() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script_path = root.join("scripts/pgo-build.sh");
    let script = fs::read_to_string(&script_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", script_path.display()));

    assert!(
        !script.contains("/tmp/rax-pgo-data"),
        "PGO profile paths must not use a predictable shared /tmp name"
    );
    assert!(
        !script.contains("rm -rf \"$PROF\""),
        "PGO cleanup must not delete a predictable profile path"
    );
    assert!(
        script.contains("mktemp -d \"$TMP_PARENT/rax-pgo.XXXXXX\""),
        "PGO build should create a private randomized temporary directory"
    );
    assert!(
        script.contains("trap cleanup EXIT"),
        "PGO build should remove its temporary profile directory with a trap"
    );
    assert!(
        script.contains("check_private_dir \"$WORKDIR\"")
            && script.contains("check_private_dir \"$PROFILE_DIR\""),
        "PGO build should verify ownership and permissions before using profile paths"
    );
    assert!(
        script.contains("stat -f '%Lp'") && script.contains("stat -c '%a'"),
        "PGO build should verify mode 700 on BSD/macOS and GNU stat platforms"
    );
    assert!(
        script.contains("-Cprofile-generate=$PROFILE_DIR")
            && script.contains("merge -o \"$PROFILE_DATA\" \"$PROFILE_DIR\"")
            && script.contains("-Cprofile-use=$PROFILE_DATA"),
        "PGO generation, merge, and optimized rebuild should stay inside the private directory"
    );
}
