from pathlib import Path
import hashlib, json, os, subprocess, traceback

Q = Path(__file__).resolve().parent
R = Path("/Users/int/.assist-native-display-policy-20261022")
M = Q / "gre-initialize-candidate1-source.json"
F = Q / "gre-initialize-quality-v1.json"
assert not F.exists()
manifest = json.loads(M.read_bytes())
state = {
    "state": "running",
    "source-frame-sha256": hashlib.sha256(M.read_bytes()).hexdigest(),
    "steps": [],
}


def save():
    F.write_text(json.dumps(state, indent=2) + "\n")


def verify():
    for row in manifest["files"]:
        p = R / row["path"]
        b = os.readlink(p).encode() if row["kind"] == "symlink-text" else p.read_bytes()
        assert (len(b), hashlib.sha256(b).hexdigest()) == (
            row["bytes"],
            row["sha256"],
        ), row["path"]


paths = [row["path"] for row in manifest["overrides"] if row["path"].endswith(".rs")]
assert len(paths) == 7
env = dict(
    os.environ,
    CARGO_TARGET_DIR=str(R / "target/candidate1"),
    CARGO_BUILD_JOBS="2",
    CARGO_INCREMENTAL="0",
    TMPDIR=str(R / "t"),
    TMP=str(R / "t"),
    TEMP=str(R / "t"),
    CCACHE_DISABLE="1",
    SCCACHE_DISABLE="1",
)
for key in [
    "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "CCACHE_DIR",
    "SCCACHE_DIR",
]:
    env.pop(key, None)
save()
try:
    for name, argv in [
        (
            "rustfmt",
            [
                "rustup",
                "run",
                "1.95.0",
                "rustfmt",
                "--edition",
                "2024",
                "--check",
                "--config",
                "skip_children=true",
                *paths,
            ],
        ),
        (
            "strict-lib-clippy",
            [
                "cargo",
                "+1.95.0",
                "clippy",
                "--locked",
                "--offline",
                "--no-default-features",
                "--lib",
                "--",
                "-D",
                "warnings",
            ],
        ),
        (
            "all-target-clippy",
            [
                "cargo",
                "+1.95.0",
                "clippy",
                "--locked",
                "--offline",
                "--no-default-features",
                "--all-targets",
                "--features",
                "x86_64-suite",
            ],
        ),
    ]:
        verify()
        log = Q / ("gre-initialize-" + name + "-v1.log")
        assert not log.exists()
        with log.open("wb") as out:
            result = subprocess.run(
                argv, cwd=R, env=env, stdout=out, stderr=subprocess.STDOUT
            )
        b = log.read_bytes()
        state["steps"].append(
            {
                "name": name,
                "argv": argv,
                "exit-code": result.returncode,
                "log": log.name,
                "bytes": len(b),
                "sha256": hashlib.sha256(b).hexdigest(),
            }
        )
        save()
        verify()
        print(name, result.returncode, flush=True)
        assert result.returncode == 0
    state.update(state="finished", **{"exit-code": 0, "owned-rust-files": 7})
except BaseException:
    state.update(state="failed", error=traceback.format_exc(), **{"exit-code": 1})
finally:
    save()
raise SystemExit(state["exit-code"])
