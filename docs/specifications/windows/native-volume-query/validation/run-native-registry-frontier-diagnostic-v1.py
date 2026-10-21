from pathlib import Path
import datetime, hashlib, json, os, shutil, subprocess, time, traceback

B = Path("C:/Windows/Temp/assist-native-rax-20261010")
S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
R = B / "src-volume-query-candidate1"
ROOT = B / "assist"
RECORD = S / "native-registry-frontier-diagnostic-v1.json"
assert not RECORD.exists()
state = {
    "state": "waiting-for-owning-archives",
    "pid": os.getpid(),
    "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "steps": [],
    "outcomes": [],
}


def save():
    RECORD.write_text(json.dumps(state, indent=2) + "\n")


def identity(path):
    h = hashlib.sha256()
    n = 0
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1048576), b""):
            h.update(chunk)
            n += len(chunk)
    return {"path": str(path), "bytes": n, "sha256": h.hexdigest()}


def run(name, argv, env):
    log = S / ("native-registry-frontier-diagnostic-v1-" + name + ".log")
    started = time.monotonic()
    with log.open("wb") as out:
        child = subprocess.Popen(
            argv, cwd=ROOT, env=env, stdout=out, stderr=subprocess.STDOUT
        )
        state.update({"state": "running", "current-step": name, "child-pid": child.pid})
        save()
        code = child.wait()
    state["steps"].append(
        {
            "name": name,
            "argv": argv,
            "seconds": time.monotonic() - started,
            "exit-code": code,
            "log": log.name,
            **{k: v for k, v in identity(log).items() if k != "path"},
        }
    )
    save()
    assert code == 0, name
    return log


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
        f = S / "volume-query-native-owning-candidate1.json"
        if f.exists():

            try:
                row = json.loads(f.read_text())
            except ValueError:
                time.sleep(1)
                continue
            assert row["state"] != "failed", row.get(
                "error", "owning validation failed"
            )
            if row["state"] == "finished":
                assert row["exit-code"] == 0
                break
        assert time.monotonic() < deadline
        time.sleep(3)
    archives = json.loads(
        (
            S / "native-volume-query-candidate1-final-owning-archive-identities.json"
        ).read_text()
    )
    core = Path(
        next(x["path"] for x in archives["archives"] if x["path"].endswith(".rlib"))
    )
    expected = next(x for x in archives["archives"] if x["path"] == str(core))
    state["core-before"] = identity(core)
    assert (
        state["core-before"]["sha256"] == expected["sha256"]
        and state["core-before"]["bytes"] == expected["bytes"]
    )
    manifest = json.loads(
        (S / "volume-query-owning-source-hashes-candidate1.json").read_text()
    )

    def verify_sources():
        for row in manifest["sources"]:
            actual = identity(R / row["path"])
            assert (actual["bytes"], actual["sha256"]) == (
                row["bytes"],
                row["sha256"],
            ), row["path"]

    verify_sources()
    source = (
        ROOT
        / "target/native-attributes/helpers/native-registry-frontier-diagnostic-v1.rs"
    )
    shutil.copy2(S / source.name, source)
    meta = json.loads(
        (S / "native-registry-frontier-diagnostic-v1-source.json").read_text()
    )
    state["observer-before"] = identity(source)
    assert state["observer-before"]["sha256"] == meta["sha256"]
    executable = (
        ROOT / "target/native-attributes/native-registry-frontier-diagnostic-v1.exe"
    )
    env = dict(os.environ)
    env.pop("CARGO_ENCODED_RUSTFLAGS", None)
    env.update(
        json.loads(
            (S / "mutant-native-compiler-env-epoch4.json").read_text(
                encoding="utf-8-sig"
            )
        )
    )
    env.update(
        {
            "TMP": str(ROOT / "target/native-attributes/tmp"),
            "TEMP": str(ROOT / "target/native-attributes/tmp"),
        }
    )
    run(
        "build",
        [
            str(B / "toolchain/bin/rustc.exe"),
            "--edition=2024",
            "--crate-name",
            "file_attributes_ordinary_owner",
            str(source),
            "-C",
            "target-feature=+crt-static",
            "--extern",
            "rax=" + str(core),
            "-L",
            "dependency=" + str(core.parent),
            "-o",
            str(executable),
        ],
        env,
    )
    state["executable-before"] = identity(executable)
    launch = json.loads((S / "native-test-alert-v1b-launch.json").read_text())
    modules = [Path(x["path"]) for x in launch["runtime-modules-before"]] + [
        Path("C:/Windows") / part / "version.dll" for part in ["System32", "SysWOW64"]
    ]
    state["runtime-before"] = [identity(x) for x in modules]
    assert [x["sha256"] for x in state["runtime-before"][:6]] == [
        x["sha256"] for x in launch["runtime-modules-before"]
    ]
    for name, program in [
        ("smoke", R / "tests/fixtures/user/windows/bin/arm64/smoke.exe"),
        ("whoami", Path("C:/Windows/System32/whoami.exe")),
    ]:
        outcome = {
            "name": name,
            "program-before": identity(program),
            "native-before": native(program),
        }
        log = run(name, [str(executable), str(program)], env)
        outcome.update(
            {
                "program-after": identity(program),
                "native-after": native(program),
                "log": log.name,
                "log-sha256": identity(log)["sha256"],
            }
        )
        assert (
            outcome["program-before"] == outcome["program-after"]
            and outcome["native-before"] == outcome["native-after"]
        )
        state["outcomes"].append(outcome)
        save()
    state["core-after"] = identity(core)
    state["observer-after"] = identity(source)
    state["executable-after"] = identity(executable)
    state["runtime-after"] = [identity(x) for x in modules]
    assert (
        state["core-before"] == state["core-after"]
        and state["observer-before"] == state["observer-after"]
        and state["executable-before"] == state["executable-after"]
        and state["runtime-before"] == state["runtime-after"]
    )
    verify_sources()
    state.update(
        {
            "state": "finished",
            "exit-code": 0,
            "source-inputs": len(manifest["sources"]),
            "scope": "Actual owning core ordinary execution and exact native controls. Guest terminal records determine success or unsupported frontier; observer exit0 alone is not guest success.",
        }
    )
except BaseException:
    state.update({"state": "failed", "exit-code": 1, "error": traceback.format_exc()})
finally:
    state["completed-utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
print(json.dumps(state), flush=True)
raise SystemExit(state["exit-code"])
