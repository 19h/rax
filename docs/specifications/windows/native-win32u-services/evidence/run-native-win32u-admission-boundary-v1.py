from pathlib import Path
import datetime
import hashlib
import json
import os
import shutil
import subprocess
import traceback

B = Path("C:/Windows/Temp/assist-native-rax-20261010")
R = B / "src-display-candidate1"
ROOT = B / "assist"
S = R / "target/win32u-validation-recovery-v2/transfer"
F = S / "native-win32u-admission-boundary-v1.json"
OUT = ROOT / "target/native-attributes/win32u-admission-boundary-v1"
assert not F.exists() and not OUT.exists()
state = {"state": "starting", "pid": os.getpid(), "steps": []}


def save():
    F.write_text(json.dumps(state, indent=2) + "\n")


def identity(path):
    data = path.read_bytes()
    return {
        "path": str(path),
        "bytes": len(data),
        "sha256": hashlib.sha256(data).hexdigest(),
    }


def run(name, args, env):
    path = S / ("native-win32u-admission-boundary-v1-" + name + ".log")
    with path.open("wb") as output:
        child = subprocess.Popen(
            args, cwd=ROOT, env=env, stdout=output, stderr=subprocess.STDOUT
        )
        state.update(state="running", step=name, child=child.pid)
        save()
        code = child.wait()
    row = {
        "name": name,
        "argv": args,
        "exit-code": code,
        "log": path.name,
        **{k: v for k, v in identity(path).items() if k != "path"},
    }
    state["steps"].append(row)
    save()
    assert code == 0, row


save()
try:
    archives = json.loads(
        (
            S / "native-win32u-services-candidate1-final-owning-archive-identities.json"
        ).read_text()
    )
    selected = next(
        row for row in archives["archives"] if row["path"].endswith(".rlib")
    )
    core = Path(selected["path"])
    state["core-before"] = identity(core)
    assert (selected["bytes"], selected["sha256"]) == (
        state["core-before"]["bytes"],
        state["core-before"]["sha256"],
    )
    manifest = json.loads(
        (S / "win32u-services-owning-source-hashes-candidate2.json").read_text()
    )

    def verify_sources():
        for row in manifest["sources"]:
            current = identity(R / row["path"])
            assert (current["bytes"], current["sha256"]) == (
                row["bytes"],
                row["sha256"],
            )

    verify_sources()
    runtime = json.loads((S / "win32u-runtime-inputs-v2.json").read_text())
    state["runtime-before"] = [
        identity(Path(row["path"])) for row in runtime["runtime-inputs"]
    ]
    assert state["runtime-before"] == runtime["runtime-inputs"]
    OUT.mkdir()
    (OUT / "tmp").mkdir()
    source = OUT / "native-win32u-admission-boundary-v1.rs"
    shutil.copy2(S / source.name, source)
    meta = json.loads(
        (S / "native-win32u-admission-boundary-v1-source.json").read_text()
    )
    state["source-before"] = identity(source)
    assert (meta["bytes"], meta["sha256"]) == (
        state["source-before"]["bytes"],
        state["source-before"]["sha256"],
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
        TMP=str(OUT / "tmp"),
        TEMP=str(OUT / "tmp"),
        TMPDIR=str(OUT / "tmp"),
        CCACHE_DISABLE="1",
        SCCACHE_DISABLE="1",
    )
    executable = OUT / "boundary.exe"
    run(
        "build",
        [
            str(B / "toolchain/bin/rustc.exe"),
            "--edition=2024",
            "--crate-name",
            "win32u_admission_boundary",
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
    run("cases", [str(executable), str(R), str(OUT / "fixtures")], env)
    state["executable-after"] = identity(executable)
    state["source-after"] = identity(source)
    state["core-after"] = identity(core)
    state["runtime-after"] = [
        identity(Path(row["path"])) for row in runtime["runtime-inputs"]
    ]
    for key in ["source", "core", "executable", "runtime"]:
        assert state[key + "-before"] == state[key + "-after"]
    verify_sources()
    state.update(
        state="finished",
        **{
            "exit-code": 0,
            "source-inputs": len(manifest["sources"]),
            "scope": "Installed-library mode via the actual owning core: selected native leaf names NtGdiInit2; complete supplied/file DLLs and mutated guest stubs cannot admit their forged service number; native system DLL bytes unchanged.",
        }
    )
except BaseException:
    state.update(state="failed", **{"exit-code": 1, "error": traceback.format_exc()})
finally:
    state["utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
print(
    json.dumps(
        {k: v for k, v in state.items() if k not in ["runtime-before", "runtime-after"]}
    ),
    flush=True,
)
raise SystemExit(state["exit-code"])
