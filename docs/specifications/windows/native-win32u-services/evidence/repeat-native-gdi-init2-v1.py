from pathlib import Path
import hashlib, json, os, subprocess, traceback

S = Path(r"C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
F = S / "native-gdi-init2-repeat-v1.json"
assert not F.exists()


def identity(p):
    b = p.read_bytes()
    return {"path": str(p), "bytes": len(b), "sha256": hashlib.sha256(b).hexdigest()}


original = json.loads((S / "native-gdi-init2-v3-four-views-launch.json").read_bytes())
state = {
    "state": "running",
    "steps": [],
    "runtime-before": [
        identity(Path(r["path"])) for r in original["runtime-modules-before"]
    ],
}
assert state["runtime-before"] == original["runtime-modules-before"]


def save():
    F.write_text(json.dumps(state, indent=2) + "\n")


def fields(line):
    return dict(word.split("=", 1) for word in line.split()[1:])


save()
try:
    for outcome in original["outcomes"]:
        profile = outcome["profile"]
        exe = Path(outcome["executable-before"]["path"])
        before = identity(exe)
        assert before == outcome["executable-before"]
        log = S / ("native-gdi-init2-repeat-v1-" + profile + ".log")
        assert not log.exists()
        with log.open("wb") as output:
            child = subprocess.run(
                [str(exe), profile],
                cwd=exe.parent,
                stdout=output,
                stderr=subprocess.STDOUT,
            )
        assert child.returncode == 0
        after = identity(exe)
        assert before == after
        import re

        rows = re.findall(
            r"call=([^ ]+) index=(\d+) tid=(\d+) return=([0-9a-f]+) error=([0-9a-f]+)",
            log.read_text(),
        )
        old = re.findall(
            r"call=([^ ]+) index=(\d+) tid=(\d+) return=([0-9a-f]+) error=([0-9a-f]+)",
            (S / outcome["log"]).read_text(),
        )
        assert len(rows) == len(old) == 32
        assert len({x[3] for x in rows}) == len({x[3] for x in old}) == 1
        assert all(x[4] == "12345678" for x in rows + old)
        assert len({x[2] for x in rows}) == 2
        assert len({x[0] for x in rows}) == 4
        assert {x[0] for x in rows} == {x[0] for x in old}
        assert (
            rows[0][3] != old[0][3]
        ), "independent process cookie collision or wrong producer instance"
        state["steps"].append(
            {
                "profile": profile,
                "exit-code": child.returncode,
                "log": identity(log),
                "executable-before": before,
                "executable-after": after,
                "calls": 32,
                "stable-across-threads-and-phases": True,
                "independent-process-value-differs": True,
                "last-error-preserved": True,
            }
        )
        save()
    state["runtime-after"] = [
        identity(Path(r["path"])) for r in original["runtime-modules-before"]
    ]
    assert state["runtime-before"] == state["runtime-after"]
    state.update(state="finished", **{"exit-code": 0})
except BaseException:
    state.update(state="failed", error=traceback.format_exc(), **{"exit-code": 1})
finally:
    save()
raise SystemExit(state["exit-code"])
