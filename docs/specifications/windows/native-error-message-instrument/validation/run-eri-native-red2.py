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
BASE = B / "src-alpc-candidate2"
R = B / "src-eri-red2"
F = S / "eri-native-red2.json"
assert not F.exists() and not R.exists()
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
    log = S / ("eri-native-red2-" + name + ".log")
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
    meta = json.loads((S / "eri-native-red2-source.json").read_text())
    ledger = S / meta["base-native-source"]
    assert identity(ledger)["sha256"] == meta["base-native-source-sha256"]
    manifest = json.loads(ledger.read_text())
    overrides = {row["path"]: row for row in meta["overrides"]}
    inputs = meta["files"]
    archive = S / meta["archive"]
    assert pair(identity(archive)) == pair(meta)
    state["disk-before"] = {"free-bytes": shutil.disk_usage(B).free}
    save()
    R.mkdir()
    for row in manifest["files"]:
        name = row["path"]
        if name in overrides:
            continue
        target = R / name
        target.parent.mkdir(parents=True, exist_ok=True)
        os.link(BASE / name, target)
    with tarfile.open(archive) as package:
        assert {item.name for item in package.getmembers()} == set(overrides)
        for item in package.getmembers():
            assert item.isfile() and item.name in overrides
            target = R / item.name
            target.parent.mkdir(parents=True, exist_ok=True)
            with package.extractfile(item) as source, target.open("xb") as destination:
                shutil.copyfileobj(source, destination)
    verify()
    state["source-inputs"] = len(inputs)
    state["overlay"] = meta
    env = dict(os.environ)
    for key in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"]:
        env.pop(key, None)
    env.update(
        json.loads(
            (S / "mutant-native-compiler-env-epoch4.json").read_text(
                encoding="utf-8-sig"
            )
        )
    )
    (R / "target/tmp").mkdir(parents=True)
    env.update(
        CARGO_HOME=str(B / "cargo-home"),
        CARGO_TARGET_DIR=str(R / "target/normal-candidate2"),
        CARGO_BUILD_JOBS="2",
        CARGO_INCREMENTAL="0",
        RUSTC=str(B / "toolchain/bin/rustc.exe"),
        TMP=str(R / "target/tmp"),
        TEMP=str(R / "target/tmp"),
    )
    cargo = str(B / "toolchain/bin/cargo.exe")
    state["dependency-cache-source"] = str(
        B / "src-alpc-candidate2/target/normal-candidate2"
    )
    shutil.copytree(
        B / "src-alpc-candidate2/target/normal-candidate2",
        R / "target/normal-candidate2",
        ignore=shutil.ignore_patterns("rax-*", "librax*", "rax.*", "rax_*"),
    )
    run("toolchain", [cargo, "--version"])
    run("clean-core", [cargo, "clean", "--offline", "-p", "rax"])
    content = run(
        "test-build",
        [
            cargo,
            "test",
            "--locked",
            "--offline",
            "--no-default-features",
            "--lib",
            "--no-run",
            "--message-format=json",
        ],
    )
    selected = artifact(content, True)
    unit = Path(selected["executable"])
    state["test-artifact"] = selected
    state["test-image-before"] = identity(unit)
    content = run_allow_failure("red", [str(unit), "installed_error_message_instrument_registry_snapshot", "--nocapture", "--test-threads=1"])
    assert state["steps"][-1]["exit-code"] == 101 and "0 passed; 1 failed;" in content
    state["test-image-after"] = identity(unit)
    assert state["test-image-after"] == state["test-image-before"]
    verify()
    state.update(state="finished", **{"exit-code": 0, "observed-test-exit": 101, "scope": "Native installed-registry source-bound red: published implementation does not select the Error Message Instrument root. Existing host absence is independently confirmed in the same test."})
except BaseException:
    state.update(state="failed", **{"exit-code": 1, "error": traceback.format_exc()})
finally:
    state["completed-utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
print(state["state"], flush=True)
raise SystemExit(state["exit-code"])
