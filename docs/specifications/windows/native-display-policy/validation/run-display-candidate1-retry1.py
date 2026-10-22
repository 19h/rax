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
F = S / "display-candidate1-windows-retry1.json"
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
    log = S / ("display-candidate1-retry1-" + name + ".log")
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
    retained = json.loads((S / "display-cache-base-retention-retry1.json").read_text())
    assert retained["state"] == "finished" and len(retained["files"]) == 2
    for row in retained["files"]:
        assert identity(Path(row["retained"]["path"])) == row["retained"]
    meta = json.loads((S / "display-candidate1-source.json").read_text())
    ledger = S / meta["base-native-source"]
    assert identity(ledger)["sha256"] == meta["base-native-source-sha256"]
    manifest = json.loads(ledger.read_text())
    overrides = {row["path"]: row for row in meta["overrides"]}
    inputs = meta["files"]
    archive = S / meta["archive"]
    assert pair(identity(archive)) == pair(meta)
    state["disk-before"] = {"free-bytes": shutil.disk_usage(B).free}
    save()
    cleanup = json.loads((S / "display-abandoned-build-cleanup-v2.json").read_text())
    assert cleanup["state"] == "finished" and len(cleanup["deleted"]) == 26
    state["previous-attempt"] = (
        "display-candidate1-windows.json: LNK1180 insufficient disk space during test-build"
    )
    state["cleanup"] = "display-abandoned-build-cleanup-v2.json"
    verify()
    state["source-inputs"] = len(inputs)
    state["overlay"] = meta
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
        CARGO_BUILD_JOBS="1",
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
    run("clean-core", [cargo, "clean", "--offline", "-p", "rax"])
    run(
        "clean-relocated-native-dependency",
        [cargo, "clean", "--offline", "-p", "zstd-sys"],
    )
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
        "focus", [str(unit), "display_policy", "--nocapture", "--test-threads=1"]
    )
    assert "7 passed; 0 failed;" in content
    run(
        "cpu-adapter",
        [str(unit), "user::windows::arch::tests", "--nocapture", "--test-threads=1"],
    )
    content = run_allow_failure("full", [str(unit), "--nocapture", "--test-threads=1"])
    existing = set(
        json.loads((S / "mutant-native-baseline-failures.json").read_text())["failures"]
    )
    failures = set(
        re.findall(r"^    (\S+)\r?$", content[content.rfind("\nfailures:\n") :], re.M)
    )
    assert state["steps"][-1]["exit-code"] == 101 and failures == existing
    assert "7648 passed; 5 failed; 2 ignored; 0 measured; 0 filtered out;" in content
    state["exact-existing-failures"] = sorted(failures)
    for name, args in [
        ("capi", ["test", "-p", "rax-capi"]),
        (
            "integration",
            ["test", "--test", "user_windows", "--test", "user_windows_memory"],
        ),
        ("all-targets", ["check", "--all-targets"]),
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
        content = run(name, argv)
        if name == "capi":
            assert "183 passed; 0 failed;" in content
        if name == "integration":
            assert "4 passed; 0 failed;" in content
        verify()
    state["test-image-after"] = identity(unit)
    assert state["test-image-before"] == state["test-image-after"]
    content = run(
        "core-build",
        [
            cargo,
            "build",
            "--locked",
            "--offline",
            "--no-default-features",
            "--lib",
            "--message-format=json",
        ],
    )
    core_row = artifact(content, False)
    cores = [Path(name) for name in core_row["filenames"] if name.endswith(".rlib")]
    assert len(cores) == 1
    core = cores[0]
    state["core-artifact"] = core_row
    state["core-before"] = identity(core)
    observer = S / "file-section-candidate1-observer.rs"
    state["observer-before"] = identity(observer)
    executable = R / "target/display-candidate1-retry1-observer.exe"
    run(
        "observer-build",
        [
            str(B / "toolchain/bin/rustc.exe"),
            "--edition=2024",
            "--crate-name",
            "file_section_candidate1",
            str(observer),
            "--extern",
            "rax=" + str(core),
            "-L",
            "dependency=" + str(core.parent),
            "-L",
            "dependency=" + str(core.parent / "deps"),
            "-o",
            str(executable),
        ],
    )
    state["executable-before"] = identity(executable)
    launch = json.loads((S / "native-test-alert-v1b-launch.json").read_text())
    modules = [Path(x["path"]) for x in launch["runtime-modules-before"]] + [
        Path("C:/Windows") / part / "version.dll" for part in ["System32", "SysWOW64"]
    ]
    state["runtime-before"] = [identity(x) for x in modules]
    state["outcomes"] = []
    for name, program in [
        ("smoke", R / "tests/fixtures/user/windows/bin/arm64/smoke.exe"),
        ("whoami", Path("C:/Windows/System32/whoami.exe")),
    ]:
        row = {
            "name": name,
            "program-before": identity(program),
            "native-before": native(program),
        }
        run(name, [str(executable), str(program)])
        row.update(
            {"program-after": identity(program), "native-after": native(program)}
        )
        assert (
            row["program-before"] == row["program-after"]
            and row["native-before"] == row["native-after"]
        )
        state["outcomes"].append(row)
        save()
    for name, path in [
        ("core", core),
        ("observer", observer),
        ("executable", executable),
    ]:
        state[name + "-after"] = identity(path)
        assert state[name + "-before"] == state[name + "-after"]
    state["runtime-after"] = [identity(x) for x in modules]
    assert state["runtime-before"] == state["runtime-after"]
    verify()
    state.update(
        state="finished",
        **{
            "exit-code": 0,
            "scope": "Display candidate1 frozen source: focused/full/CAPI/integration/all-target and selected ordinary core. Actual owning Assist carrier is a separate gate. Guest terminal record determines success/frontier; observer exit0 is not guest exit.",
        }
    )
except BaseException:
    state.update(state="failed", **{"exit-code": 1, "error": traceback.format_exc()})
finally:
    state["completed-utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
print(json.dumps(state), flush=True)
raise SystemExit(state["exit-code"])
