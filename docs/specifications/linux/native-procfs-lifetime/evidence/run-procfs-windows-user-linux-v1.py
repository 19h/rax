"""Isolated Windows prototype: immutable overlay, selected artifacts and runs."""

from concurrent.futures import ThreadPoolExecutor
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import time
import traceback

B = Path("C:/Windows/Temp/assist-native-rax-20261010")
S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
BASE = B / "src-eri-candidate1"
R = B / "src-display-candidate1"
F = S / "procfs-windows-user-linux-v1.json"
assert not F.exists() and R.is_dir()
state = {
    "state": "preparing",
    "pid": os.getpid(),
    "steps": [],
    "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
}


def save():
    F.write_text(json.dumps(state, indent=2) + "\n")


def identity(path):
    h = hashlib.sha256()
    n = 0
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            h.update(block)
            n += len(block)
    return {"path": str(path), "bytes": n, "sha256": h.hexdigest()}


def pair(row):
    return row["bytes"], row["sha256"]


def verify():
    def check(row):
        expected = pair(row)
        if row["path"] == ".cargo/config.toml":
            assert expected == (
                552,
                "a198c84574684e95de13dbb1bdb7b6c918f90fdb490c649937f697fc9621f3cd",
            )
            expected = (
                374,
                "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
            )
        assert pair(identity(R / row["path"])) == expected, row["path"]

    with ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(check, inputs))


def run_allow_failure(name, argv):
    log = S / ("procfs-windows-user-linux-v1-" + name + ".log")
    start = time.monotonic()
    with log.open("wb") as stream:
        child = subprocess.Popen(
            argv, cwd=R, env=env, stdout=stream, stderr=subprocess.STDOUT
        )
        state.update(state="running", **{"current-step": name, "child-pid": child.pid})
        save()
        code = child.wait()
    content = log.read_text(errors="replace")
    state["steps"].append(
        {
            "name": name,
            "argv": argv,
            "exit-code": code,
            "seconds": time.monotonic() - start,
            "log": log.name,
            **{k: v for k, v in identity(log).items() if k != "path"},
            "results": re.findall(r"test result: ([^\r\n]+)", content),
        }
    )
    save()
    return content


def run(name, argv):
    content = run_allow_failure(name, argv)
    assert state["steps"][-1]["exit-code"] == 0, name
    return content


def artifact(content, test):
    rows = []
    for line in content.splitlines():
        try:
            row = json.loads(line)
        except ValueError:
            continue
        if (
            row.get("reason") == "compiler-artifact"
            and row["target"]["name"] == "rax"
            and row["profile"]["test"] == test
        ):
            rows.append(row)
    assert len(rows) == 1
    row = rows[0]
    assert (
        Path(row["manifest_path"]).parent.resolve() == R.resolve()
        and row["features"] == []
    )
    return row


def native(program):
    result = subprocess.run([str(program)], capture_output=True, timeout=30)
    return {
        "exit-code": result.returncode,
        "stdout-bytes": len(result.stdout),
        "stdout-sha256": hashlib.sha256(result.stdout).hexdigest(),
        "stderr-bytes": len(result.stderr),
        "stderr-sha256": hashlib.sha256(result.stderr).hexdigest(),
    }


save()
try:
    deadline = time.monotonic() + 7200
    while True:
        p = S / "procfs-candidate1-windows.json"
        d = json.loads(p.read_bytes())
        assert d["state"] != "failed", d.get("error", "")
        if d["state"] == "finished":
            break
        assert time.monotonic() < deadline
        time.sleep(3)
    meta = json.loads((S / "procfs-candidate1-source.json").read_bytes())
    inputs = meta["files"]
    verify()
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
        json.loads(
            (S / "mutant-native-compiler-env-epoch4.json").read_text(
                encoding="utf-8-sig"
            )
        )
    )
    (R / "target/tmp").mkdir(parents=True, exist_ok=True)
    env.update(
        CARGO_HOME=str(B / "cargo-home"),
        CARGO_TARGET_DIR=str(R / "target/normal-candidate1"),
        CARGO_BUILD_JOBS="2",
        CARGO_INCREMENTAL="0",
        RUSTC=str(B / "toolchain/bin/rustc.exe"),
        TMP=str(R / "target/tmp"),
        TEMP=str(R / "target/tmp"),
        TMPDIR=str(R / "target/tmp"),
        CCACHE_DISABLE="1",
        SCCACHE_DISABLE="1",
    )
    cargo = str(B / "toolchain/bin/cargo.exe")
    state["dependency-cache-source"] = str(R / "target/normal-candidate1")
    run("toolchain", [cargo, "--version"])
    assert Path(env["CARGO_TARGET_DIR"]).resolve().is_relative_to(R.resolve())
    state["build-environment"] = {
        key: env.get(key)
        for key in [
            "CARGO_TARGET_DIR",
            "TMPDIR",
            "TMP",
            "TEMP",
            "CCACHE_DISABLE",
            "SCCACHE_DISABLE",
        ]
    }

    text = run(
        "integration",
        [
            cargo,
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
    assert "0 passed; 0 failed; 0 ignored;" in text
    verify()
    state.update(
        state="finished",
        **{
            "exit-code": 0,
            "source-inputs": len(inputs),
            "scope": "Windows compilation only: pre-existing user_linux target is cfg(unix); portable procfs runtime cases are in the full library and six focused tests.",
        }
    )
except BaseException:
    state.update(state="failed", **{"exit-code": 1, "error": traceback.format_exc()})
finally:
    state["completed-utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
print(json.dumps({k: v for k, v in state.items() if k != "steps"}), flush=True)
raise SystemExit(state["exit-code"])
