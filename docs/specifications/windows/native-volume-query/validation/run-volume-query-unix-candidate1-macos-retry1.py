"""Complete native Unix Cargo gates against one immutable source graph."""

from pathlib import Path
import datetime, hashlib, json, os, re, subprocess, sys, time, traceback

CACHE = Path(__file__).parent
ROOT = Path("/Users/int/.assist-native-volume-query-20261021")
HOST = sys.argv[1]
assert HOST in ["macos", "linux"]
RECORD = CACHE / ("volume-query-" + HOST + "-candidate1-retry1-gates.json")
assert not RECORD.exists()
manifest_path = CACHE / "volume-query-candidate1-source.json"
assert (
    hashlib.sha256(manifest_path.read_bytes()).hexdigest()
    == "336e8771aaa128fabb49c5e87cd2629285344f0b04f2de7eee78d851ceab44de"
)
manifest = json.loads(manifest_path.read_text())
state = {
    "state": "preparing",
    "source-epoch": 1,
    "host": HOST,
    "driver-pid": os.getpid(),
    "steps": [],
}


def save():
    RECORD.write_text(json.dumps(state, indent=2) + "\n")


def verify():
    for row in manifest["files"]:
        p = ROOT / row["path"]
        raw = (
            os.readlink(p).encode() if row["kind"] == "symlink-text" else p.read_bytes()
        )
        assert (len(raw), hashlib.sha256(raw).hexdigest()) == (
            row["bytes"],
            row["sha256"],
        ), row["path"]


prefix = []
env = dict(os.environ)
env.update(
    CARGO_BUILD_JOBS="2",
    CARGO_INCREMENTAL="0",
    CARGO_TARGET_DIR=str(ROOT / "target/candidate1"),
    TMPDIR=str(ROOT / "target/tmp"),
    CCACHE_DISABLE="1",
    SCCACHE_DISABLE="1",
)
cargo = ["cargo", "+1.95.0"]
if HOST == "linux":
    (ROOT / "target/linux").mkdir(parents=True, exist_ok=True)
    prefix = [
        "docker",
        "run",
        "--rm",
        "--name",
        "assist-native-volume-query-candidate1",
        "--platform",
        "linux/amd64",
        "-v",
        "/Users/int/hexrays/kvasir:/work:ro",
        "-v",
        str(ROOT) + ":/work/vendor/rax:ro",
        "-v",
        "rax-cargo-registry:/usr/local/cargo/registry",
        "-v",
        "rax-cargo-git:/usr/local/cargo/git",
        "-v",
        "rax-user-target:/work/vendor/rax/target/linux",
        "-e",
        "CARGO_TARGET_DIR=/work/vendor/rax/target/linux/volume-query-candidate1",
        "-e",
        "CARGO_BUILD_JOBS=2",
        "-e",
        "CARGO_INCREMENTAL=0",
        "-e",
        "RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu",
        "-w",
        "/work/vendor/rax",
        "assist-rax-validation-rustfmt:1.95-bullseye-20261015",
        "sh",
        "-c",
        'mkdir -p /work/vendor/rax/target/linux/tmp && export TMPDIR=/work/vendor/rax/target/linux/tmp && exec "$@"',
        "sh",
    ]
    cargo = ["cargo"]
save()
try:
    for name, args in [
        ("focus", ["test", "--lib", "volume_query_tests"]),
        ("full", ["test", "--lib"]),
        ("capi", ["test", "-p", "rax-capi"]),
        (
            "integration",
            ["test", "--test", "user_windows", "--test", "user_windows_memory"],
        ),
        ("all-targets", ["check", "--all-targets"]),
    ]:
        verify()
        argv = (
            prefix
            + cargo
            + [args[0], "--locked", "--offline", "--no-default-features", *args[1:]]
        )
        if args[0] == "test":
            argv += ["--", "--nocapture", "--test-threads=1"]
        log = CACHE / ("volume-query-" + HOST + "-candidate1-retry1-" + name + ".log")
        start = time.monotonic()
        with log.open("wb") as out:
            child = subprocess.Popen(
                argv, cwd=ROOT, env=env, stdout=out, stderr=subprocess.STDOUT
            )
            state.update(
                {"state": "running", "current-step": name, "child-pid": child.pid}
            )
            save()
            code = child.wait()
        verify()
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
        print(HOST, name, code, results, flush=True)
        assert code == 0, name
        expected = {
            "focus": 6,
            "full": 8146 if HOST == "macos" else 8140,
            "capi": 183,
            "integration": 544,
        }
        if name in expected:
            assert any(f"{expected[name]} passed; 0 failed;" in r for r in results), (
                name,
                results,
            )
        if name == "full":
            assert any("2 ignored; 0 measured; 0 filtered out;" in r for r in results)
    state.update(
        {
            "state": "finished",
            "exit-code": 0,
            "post-source-inputs": len(manifest["files"]),
        }
    )
except BaseException:
    state.update({"state": "failed", "exit-code": 1, "error": traceback.format_exc()})
finally:
    state["completed-utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
raise SystemExit(state["exit-code"])
