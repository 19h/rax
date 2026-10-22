from pathlib import Path
import hashlib, json, os, subprocess, traceback

S = Path(r"C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
F = S / "native-gre-repeat-v1.json"
assert not F.exists()


def identity(p):
    b = p.read_bytes()
    return {"path": str(p), "bytes": len(b), "sha256": hashlib.sha256(b).hexdigest()}


original = json.loads(
    (S / "native-gre-initialize-v1-four-views-launch.json").read_bytes()
)
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
        log = S / ("native-gre-repeat-v1-" + profile + ".log")
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
        rows = log.read_text().splitlines()
        old = (S / outcome["log"]).read_text().splitlines()
        tree = lambda lines: [
            line
            for line in lines
            if line.startswith(("TREE ", "VALUE ", "CHILD ", "TREE_END "))
        ]
        assert tree(rows) == tree(old)
        opens = [fields(line) for line in rows if line.startswith("OPEN ")]
        prior = [fields(line) for line in old if line.startswith("OPEN ")]
        assert len(opens) == len(prior) == 144
        for a, b in zip(opens, prior):
            handle = int(a.pop("handle"), 16)
            old_handle = int(b.pop("handle"), 16)
            assert a == b
            assert (handle != 0) == (old_handle != 0)
        summary = fields(next(line for line in rows if line.startswith("SUMMARY ")))
        assert (
            summary["calls"] == "144"
            and summary["opened"] == summary["closed"] == "128"
            and summary["handles-before"] == summary["handles-after"]
        )
        state["steps"].append(
            {
                "profile": profile,
                "exit-code": child.returncode,
                "log": identity(log),
                "executable-before": before,
                "executable-after": after,
                "read-tree-records-match": True,
                "all-open-fields-match-except-opaque-handle-number": True,
                "calls": 144,
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
