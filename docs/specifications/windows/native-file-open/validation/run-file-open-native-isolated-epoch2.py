"""Rebuild the primary crate after the shared-target baseline, then bind executions."""

from concurrent.futures import ThreadPoolExecutor
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
import traceback

B = Path("C:/Windows/Temp/assist-native-rax-20261010")
S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
R = B / "src-file-open-epoch2"
F = S / "file-open-native-epoch2-isolated-gates.json"
PREFIX = "file-open-native-epoch2-isolated-"
assert not F.exists()
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
        list(pool.map(check, manifest["files"]))


def run(name, argv):
    path = S / (PREFIX + name + ".log")
    start = time.monotonic()
    with path.open("wb") as stream:
        child = subprocess.Popen(
            argv, cwd=R, env=env, stdout=stream, stderr=subprocess.STDOUT
        )
        state.update(state="running", **{"current-step": name, "child-pid": child.pid})
        save()
        code = child.wait()
    text = path.read_text(errors="replace")
    row = {
        "name": name,
        "argv": argv,
        "exit-code": code,
        "seconds": time.monotonic() - start,
        "log": path.name,
        **{k: v for k, v in identity(path).items() if k != "path"},
        "results": re.findall(r"test result: ([^\r\n]+)", text),
    }
    state["steps"].append(row)
    save()
    return code, text


save()
try:
    previous = json.loads((S / "file-open-native-epoch2-gates.json").read_text())
    baseline = json.loads((S / "file-open-delete-baseline-unit-v3.json").read_text())
    assert previous["state"] == "failed" and baseline["state"] == "finished"
    manifest_path = S / "file-open-source-epoch2.json"
    assert (
        identity(manifest_path)["sha256"]
        == "f257d4a412aeed642d4351ca6e7d1dea89ec95173a09eb430e5036651ac625a5"
    )
    manifest = json.loads(manifest_path.read_text())
    verify()
    state["verified-source-inputs"] = len(manifest["files"])
    state["verified-source-bytes"] = manifest["total-bytes"]
    live_cmd = [
        "powershell.exe",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "@(Get-CimInstance Win32_Process | Where-Object { $_.Name -match '^(cargo|rustc|rust-lld|link)\\.exe$' -and $_.CommandLine -like '*src-file-open-epoch2*target*normal*' } | Select-Object ProcessId,Name,CommandLine) | ConvertTo-Json -Compress",
    ]
    live = subprocess.run(
        live_cmd, capture_output=True, text=True, check=True
    ).stdout.strip()
    assert live in ["", "[]"], live
    state["cache-live-references"] = []
    old_image = R / "target/normal/debug/deps/rax-b799307a7949aa6b.exe"
    retained = R / "target/retained-rejected-full-epoch2"
    retained.mkdir(exist_ok=False)
    destination = retained / "unit.exe"
    shutil.copy2(old_image, destination)
    a, b = identity(old_image), identity(destination)
    assert pair(a) == pair(b) and not old_image.samefile(destination)
    state["retained-rejected-full-image"] = {"source": a, "retained": b}
    dep = old_image.with_suffix(".d")
    if dep.exists():
        text = dep.read_text(errors="replace")
        state["prior-dependency-file"] = {
            **identity(dep),
            "baseline-root-occurrences": text.count("fo-db3"),
            "candidate-root-occurrences": text.count("src-file-open-epoch2"),
        }
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
    env.update(
        CARGO_HOME=str(B / "cargo-home"),
        CARGO_TARGET_DIR=str(R / "target/normal"),
        CARGO_BUILD_JOBS="2",
        CARGO_INCREMENTAL="0",
        RUSTC=str(B / "toolchain/bin/rustc.exe"),
        TMP=str(R / "target/tmp"),
        TEMP=str(R / "target/tmp"),
    )
    cargo = str(B / "toolchain/bin/cargo.exe")
    assert run("toolchain", [cargo, "--version"])[0] == 0
    assert run("clean-core", [cargo, "clean", "--offline", "--package", "rax"])[0] == 0
    code, text = run(
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
    assert code == 0 and "Compiling rax v0.1.0" in text
    candidates = []
    for line in text.splitlines():
        try:
            row = json.loads(line)
        except ValueError:
            continue
        if (
            row.get("reason") == "compiler-artifact"
            and row["target"]["name"] == "rax"
            and row.get("executable")
            and row["profile"]["test"]
        ):
            candidates.append(row)
    assert len(candidates) == 1
    artifact = candidates[0]
    assert (
        Path(artifact["manifest_path"]).parent.resolve() == R.resolve()
        and artifact["features"] == []
    )
    image = Path(artifact["executable"])
    selected = identity(image)
    state["selected-test-artifact"] = artifact
    state["selected-test-image"] = selected
    verify()
    existing = set(
        json.loads((S / "mutant-native-baseline-failures.json").read_text())["failures"]
    )
    for name, argv in [
        ("focus", [str(image), "native_file_", "--nocapture", "--test-threads=1"]),
        ("full", [str(image), "--nocapture", "--test-threads=1"]),
    ]:
        before = identity(image)
        assert before == selected
        code, text = run(name, argv)
        after = identity(image)
        assert after == selected
        state["steps"][-1].update(
            **{"test-image-before": before, "test-image-after": after}
        )
        verify()
        if name == "focus":
            assert code == 0 and "26 passed; 0 failed;" in text
        else:
            final = text[text.rfind("\nfailures:\n") :]
            failures = set(re.findall(r"^    (\S+)\r?$", final, re.M))
            assert code == 101 and failures == existing
            assert (
                "7590 passed; 5 failed; 2 ignored; 0 measured; 0 filtered out;" in text
            )
            state["exact-existing-failures"] = sorted(failures)
        save()
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
        code, text = run(name, argv)
        assert code == 0
        if name == "capi":
            assert "183 passed; 0 failed;" in text
        elif name == "integration":
            assert "4 passed; 0 failed;" in text
        verify()
    state.update(
        state="finished",
        **{
            "exit-code": 0,
            "post-source-inputs": len(manifest["files"]),
            "scope": "Candidate2 recompiled after primary-crate cache eviction; the actual Cargo-selected library-test executable is hashed before/after direct focused and unfiltered executions. Prior shared-target full run is rejected, not a product validation result. Dependencies reused; no baseline writer remains in this target.",
        }
    )
except BaseException:
    state.update(state="failed", **{"exit-code": 1, "error": traceback.format_exc()})
finally:
    state["completed-utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
print(json.dumps(state), flush=True)
raise SystemExit(state["exit-code"])
