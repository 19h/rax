"""Validate a frozen ProcessBasicInformation compiler graph on Unix hosts."""

from pathlib import Path
import datetime, hashlib, json, os, re, subprocess, sys, time, traceback

Q = Path(__file__).parent
R = Path("/Users/int/.assist-native-display-policy-20261022")
HOST = sys.argv[1]
assert HOST in ["macos", "linux"]
F = Q / ("procfs-" + HOST + "-user-linux-v1.json")
assert not F.exists()
M = Q / "procfs-candidate1-source.json"
assert (
    hashlib.sha256(M.read_bytes()).hexdigest()
    == "b1c920a113cb4a6e35243e907bfe72751d4e6c0a6eef67afaafa14047d7b57cb"
)
manifest = json.loads(M.read_bytes())
state = {
    "state": "preparing",
    "host": HOST,
    "pid": os.getpid(),
    "steps": [],
    "manifest-sha256": hashlib.sha256(M.read_bytes()).hexdigest(),
}


def save():
    F.write_text(json.dumps(state, indent=2) + "\n")


def verify():
    for row in manifest["files"]:
        p = R / row["path"]
        raw = (
            os.readlink(p).encode() if row["kind"] == "symlink-text" else p.read_bytes()
        )
        assert (len(raw), hashlib.sha256(raw).hexdigest()) == (
            row["bytes"],
            row["sha256"],
        ), row["path"]


env = dict(os.environ)
for key in [
    "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "CCACHE_DIR",
    "SCCACHE_DIR",
]:
    env.pop(key, None)
env.update(
    CARGO_TARGET_DIR=str(R / "target/candidate1"),
    CARGO_BUILD_JOBS="2",
    CARGO_INCREMENTAL="0",
    TMPDIR=str(R / "t"),
    TMP=str(R / "t"),
    TEMP=str(R / "t"),
    CCACHE_DISABLE="1",
    SCCACHE_DISABLE="1",
)
prefix = []
cargo = ["cargo", "+1.95.0"]
if HOST == "linux":
    root = "/work/vendor/rax"
    (R / "target/linux/tmp").mkdir(parents=True, exist_ok=True)
    prefix = [
        "docker",
        "run",
        "--rm",
        "--platform",
        "linux/amd64",
        "-v",
        str(R) + ":" + root,
        "-v",
        "assist-native-procfs-linux-tmp-20261022:/work/vendor/rax/target/linux/tmp",
        "-v",
        "rax-cargo-registry:/usr/local/cargo/registry",
        "-v",
        "rax-cargo-git:/usr/local/cargo/git",
        "-e",
        "CARGO_TARGET_DIR=" + root + "/target/linux/candidate1",
        "-e",
        "TMPDIR=" + root + "/target/linux/tmp",
        "-e",
        "TMP=" + root + "/target/linux/tmp",
        "-e",
        "TEMP=" + root + "/target/linux/tmp",
        "-e",
        "CARGO_BUILD_JOBS=2",
        "-e",
        "CARGO_INCREMENTAL=0",
        "-e",
        "CCACHE_DISABLE=1",
        "-e",
        "SCCACHE_DISABLE=1",
        "-e",
        "RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu",
        "-w",
        root,
        "assist-rax-validation-rustfmt:1.95-bullseye-20261015",
    ]
    cargo = ["cargo"]


def run(name, args):
    verify()
    argv = prefix + cargo + args
    log = Q / ("procfs-" + HOST + "-user-linux-v1-" + name + ".log")
    assert not log.exists()
    start = time.monotonic()
    with log.open("wb") as stream:
        child = subprocess.Popen(
            argv, cwd=R, env=env, stdout=stream, stderr=subprocess.STDOUT
        )
        state.update(state="running", step=name, child=child.pid)
        save()
        code = child.wait()
    raw = log.read_bytes()
    results = re.findall(r"test result: ([^\r\n]+)", raw.decode(errors="replace"))
    state["steps"].append(
        {
            "name": name,
            "argv": argv,
            "exit-code": code,
            "seconds": time.monotonic() - start,
            "log": log.name,
            "bytes": len(raw),
            "sha256": hashlib.sha256(raw).hexdigest(),
            "results": results,
        }
    )
    save()
    verify()
    print(HOST, name, code, results, flush=True)
    assert code == 0, name
    return results


save()
try:
    results = run(
        "integration",
        [
            "test",
            "--locked",
            "--offline",
            "--no-default-features",
            "--test",
            "user_linux",
            "--",
            "--nocapture",
            "--test-threads=1",
        ],
    )
    assert any(
        "passed; 0 failed;" in value and not value.startswith("ok. 0 passed;")
        for value in results
    ), results
    state.update(
        state="finished", **{"exit-code": 0, "source-inputs": len(manifest["files"])}
    )
except BaseException:
    state.update(state="failed", **{"exit-code": 1, "error": traceback.format_exc()})
finally:
    state["completed-utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
raise SystemExit(state["exit-code"])
