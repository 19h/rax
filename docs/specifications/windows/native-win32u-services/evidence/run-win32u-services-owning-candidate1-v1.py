from pathlib import Path
import datetime, hashlib, json, os, subprocess, sys, time, traceback

P = Path(__file__).parent
R = Path("/Users/int/.assist-native-display-policy-20261022")
HOST = sys.argv[1]
assert HOST in ["macos", "linux"]
RECORD = P / (HOST + "-win32u-services-candidate1-owning-gates-v1.json")
assert not RECORD.exists()
manifest = json.loads(
    (P / "win32u-services-owning-source-hashes-candidate1.json").read_text()
)
commands = json.loads(
    (P / (HOST + "-win32u-services-candidate1-owning-commands-v1.json")).read_text()
)
state = {
    "state": "running",
    "host": HOST,
    "pid": os.getpid(),
    "source-inputs": len(manifest["sources"]),
    "scope": "Retained Assist ABI184 linked consumers; no whole current root GUI/plugin/package claim.",
    "steps": [],
}


def save():
    RECORD.write_text(json.dumps(state, indent=2) + "\n")


def verify():
    for row in manifest["sources"]:
        raw = (R / row["path"]).read_bytes()
        assert (
            len(raw) == row["bytes"]
            and hashlib.sha256(raw).hexdigest() == row["sha256"]
        ), row["path"]
    for row in manifest["helpers"]:
        raw = Path(row["local-path"]).read_bytes()
        assert (len(raw), hashlib.sha256(raw).hexdigest()) == (
            row["bytes"],
            row["sha256"],
        ), row["path"]


save()
try:
    for row in commands:
        verify()
        log = P / (
            HOST + "-win32u-services-candidate1-owning-v1-" + row["gate"] + ".log"
        )
        started = time.monotonic()
        env = dict(os.environ, RUSTUP_TOOLCHAIN="1.95.0")
        for key in [
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "CCACHE_DIR",
            "SCCACHE_DIR",
        ]:
            env.pop(key, None)
        env.update(CCACHE_DISABLE="1", SCCACHE_DISABLE="1")
        if HOST == "macos":
            env["TMPDIR"] = str(
                P.parent
                / "wait-completion-packet-create-20261012/assist-root-current/build-native-attributes/tmp"
            )
            env["TMP"] = env["TMPDIR"]
            env["TEMP"] = env["TMPDIR"]
            assert (
                Path(env["TMPDIR"])
                .resolve()
                .is_relative_to(Path(commands[0]["cwd"]).parents[1].resolve())
            )
        with log.open("wb") as out:
            child = subprocess.Popen(
                row["command"],
                cwd=row.get("cwd"),
                env=env,
                stdout=out,
                stderr=subprocess.STDOUT,
            )
            state.update({"current-step": row["gate"], "child-pid": child.pid})
            save()
            code = child.wait()
        verify()
        raw = log.read_bytes()
        state["steps"].append(
            {
                **row,
                "exit-code": code,
                "seconds": time.monotonic() - started,
                "log": log.name,
                "bytes": len(raw),
                "sha256": hashlib.sha256(raw).hexdigest(),
            }
        )
        save()
        print(HOST, row["gate"], code, flush=True)
        assert code == 0, row["gate"]
    a = json.loads(
        (P / (HOST + "-win32u-services-candidate1-archives-before.json")).read_text()
    )
    b = json.loads(
        (P / (HOST + "-win32u-services-candidate1-archives-after.json")).read_text()
    )
    assert a["archives"] == b["archives"]
    state.update(
        {"state": "finished", "exit-code": 0, "archive-identities-stable": True}
    )
except BaseException:
    state.update({"state": "failed", "exit-code": 1, "error": traceback.format_exc()})
finally:
    state["completed-utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    save()
raise SystemExit(state["exit-code"])
