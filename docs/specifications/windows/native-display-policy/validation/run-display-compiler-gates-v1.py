from pathlib import Path
import datetime, hashlib, json, os, subprocess, time, traceback

Q = Path(__file__).parent
W = Path("/Users/int/.assist-native-display-policy-20261022")
F = Q / "display-compiler-gates-v1.json"
assert not F.exists()
meta = Q / "display-candidate1-source.json"
inputs = json.loads(meta.read_text())["files"]
state = {
    "state": "running",
    "pid": os.getpid(),
    "source-ledger-sha256": hashlib.sha256(meta.read_bytes()).hexdigest(),
    "steps": [],
}


def save():
    F.write_text(json.dumps(state, indent=2) + "\n")


def verify():
    for row in inputs:
        p = W / row["path"]
        raw = (
            os.readlink(p).encode() if row["kind"] == "symlink-text" else p.read_bytes()
        )
        assert (len(raw), hashlib.sha256(raw).hexdigest()) == (
            row["bytes"],
            row["sha256"],
        ), row["path"]


tmp = W / "t"
tmp.mkdir(exist_ok=True)
env = dict(
    os.environ,
    CARGO_TARGET_DIR=str(W / "target/candidate1"),
    CARGO_BUILD_JOBS="2",
    CARGO_INCREMENTAL="0",
    TMPDIR=str(tmp),
    TMP=str(tmp),
    TEMP=str(tmp),
    CCACHE_DISABLE="1",
    SCCACHE_DISABLE="1",
)
for key in ["RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "CCACHE_DIR", "SCCACHE_DIR"]:
    env.pop(key, None)
owned = [
    "src/user/windows/registry.rs",
    "src/user/windows/registry/tree.rs",
    "src/user/windows/registry/installed.rs",
    "src/user/windows/registry/display_policy_tests.rs",
    "src/user/windows/registry/installed/display_policy_tests.rs",
    "src/user/windows/process/sched/tests/services_tests/registry_tests.rs",
    "src/user/windows/process/sched/tests/services_tests/registry_tests/display_policy.rs",
]
commands = [
    (
        "format",
        [
            "rustfmt",
            "+1.95.0",
            "--edition",
            "2024",
            "--config",
            "skip_children=true",
            "--check",
            *owned,
        ],
    ),
    (
        "clippy",
        [
            "cargo",
            "+1.95.0",
            "clippy",
            "--locked",
            "--offline",
            "--all-targets",
            "--features",
            "x86_64-suite",
        ],
    ),
]

save()
try:
    for name, args in commands:
        verify()
        log = Q / ("display-compiler-" + name + "-v1.log")
        start = time.monotonic()
        with log.open("wb") as out:
            child = subprocess.Popen(
                args, cwd=W, env=env, stdout=out, stderr=subprocess.STDOUT
            )
            state.update(current=name, child_pid=child.pid)
            save()
            code = child.wait()
        verify()
        raw = log.read_bytes()
        state["steps"].append(
            {
                "name": name,
                "argv": args,
                "exit-code": code,
                "log": log.name,
                "bytes": len(raw),
                "sha256": hashlib.sha256(raw).hexdigest(),
                "seconds": time.monotonic() - start,
            }
        )
        save()
        assert code == 0, name
    state.update(state="finished", exit_code=0)
except BaseException:
    state.update(state="failed", exit_code=1, error=traceback.format_exc())
state["completed-utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
save()
print(json.dumps({k: v for k, v in state.items() if k != "steps"}))
raise SystemExit(state["exit_code"])
