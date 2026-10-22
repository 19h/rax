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
F = S / "read-memory-candidate2-windows.json"
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
    log = S / ("read-memory-candidate2-" + name + ".log")
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
    mf = S / "read-memory-candidate2-source.json"
    assert (
        identity(mf)["sha256"]
        == "c6f269d084f9391ff8913de2c0a19cfbdc95de5412ecf1abe2bab9d979282f46"
    )
    meta = json.loads(mf.read_bytes())
    inputs = meta["files"]
    old = json.loads((S / "read-memory-candidate1-source.json").read_bytes())
    for row in old["files"]:
        expected = pair(row)
        if row["path"] == ".cargo/config.toml":
            expected = (
                374,
                "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
            )
        assert pair(identity(R / row["path"])) == expected, row["path"]
    archive = S / meta["archive"]
    assert pair(identity(archive)) == pair(meta)
    temp = R / "target/read-memory-materialization-v1"
    temp.mkdir(exist_ok=False)
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        expected = {x["path"]: x for x in meta["overrides"]}
        assert {m.name for m in members} == set(expected)
        for index, member in enumerate(members):
            assert (
                member.isfile()
                and not Path(member.name).is_absolute()
                and ".." not in Path(member.name).parts
            )
            raw = bundle.extractfile(member).read()
            assert (len(raw), hashlib.sha256(raw).hexdigest()) == pair(
                expected[member.name]
            )
            dst = R / member.name
            dst.parent.mkdir(parents=True, exist_ok=True)
            stage = temp / str(index)
            stage.write_bytes(raw)
            os.replace(stage, dst)
    temp.rmdir()
    verify()
    state["source-inputs"] = len(inputs)
    state["manifest"] = identity(mf)
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
    content = run(
        "focus",
        [
            str(unit),
            "user::windows::process::sched::tests::services_tests::virtual_memory_tests::read::",
            "--nocapture",
            "--test-threads=1",
        ],
    )
    assert "9 passed; 0 failed; 0 ignored;" in content
    verify()
    content = run_allow_failure("full", [str(unit), "--nocapture", "--test-threads=1"])
    existing = set(
        json.loads((S / "mutant-native-baseline-failures.json").read_text())["failures"]
    )
    failures = set(
        re.findall(r"^    (\S+)\r?$", content[content.rfind("\nfailures:\n") :], re.M)
    )
    state["exact-full-failures"] = sorted(failures)
    save()
    assert state["steps"][-1]["exit-code"] == 101 and failures == existing
    assert "7664 passed; 5 failed; 2 ignored; 0 measured; 0 filtered out;" in content
    for name, args, count in [
        ("capi", ["test", "-p", "rax-capi"], 183),
        (
            "integration",
            ["test", "--test", "user_windows", "--test", "user_windows_memory"],
            4,
        ),
        ("all-targets", ["check", "--all-targets"], None),
    ]:
        argv = [
            cargo,
            args[0],
            "--locked",
            "--offline",
            "--no-default-features",
            *args[1:],
        ]
        if args[0] == "test":
            argv += ["--", "--nocapture", "--test-threads=1"]
        text = run(name, argv)
        if count is not None:
            assert f"{count} passed; 0 failed;" in text
        verify()
    state["test-image-after"] = identity(unit)
    assert state["test-image-before"] == state["test-image-after"]
    verify()
    state.update(
        state="finished",
        **{
            "exit-code": 0,
            "scope": "Frozen NtReadVirtualMemory source: native Windows ARM64 focused/full/CAPI/integration/all-target. Existing five lowerer failures are explicit; full suite is not green.",
        },
    )
except BaseException:
    state.update(state="failed", **{"exit-code": 1, "error": traceback.format_exc()})
finally:
    state["completed-utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
print(json.dumps({k: v for k, v in state.items() if k != "steps"}), flush=True)
raise SystemExit(state["exit-code"])
