"""Rehash the union of native-producer and actual owning runtime inputs."""

from pathlib import Path
import datetime
import hashlib
import json
import sys
import traceback

S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
F = S / "process-mitigation-final-native-audit-v2.json"
assert not F.exists()
a = {
    "state": "preparing",
    "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
}


def identity(p):
    h = hashlib.sha256()
    n = 0
    with p.open("rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
            n += len(b)
    return {"path": str(p), "bytes": n, "sha256": h.hexdigest()}


def same(row):
    actual = identity(Path(row["path"]))
    assert actual == row, row["path"]
    return actual


try:
    native = json.loads(
        (S / "native-process-mitigation-v4-four-views-launch.json").read_text()
    )
    ordinary = json.loads(
        (S / "process-mitigation-native-ordinary-candidate2-retry2.json").read_text()
    )
    capi = json.loads((S / "process-mitigation-owning-capi-candidate2-retry2.json").read_text())
    gates = json.loads((S / "process-mitigation-candidate2-windows-retry2.json").read_text())
    assert all(
        x["state"] == "finished" and x["exit-code"] == 0
        for x in [native, ordinary, capi, gates]
    )
    union = {}
    for group in [
        native["runtime-modules-before"],
        ordinary["runtime-before"],
        capi["runtime-before"],
    ]:
        for row in group:
            key = row["path"].casefold()
            if key in union:
                assert union[key] == row
            union[key] = row
    assert len(union) == 10
    a["runtime-module-union"] = [same(x) for x in union.values()]
    producer_files = []
    for version in [1,2,4]:
        record = json.loads((S / ("native-process-mitigation-v"+str(version)+"-four-views-launch.json")).read_text())
        assert record["state"] == "finished" and record["exit-code"] == 0
        producer_files.append(same(record["source-after"]))
        producer_files.extend(same(row["executable-after"]) for row in record["outcomes"])
    a["native-producer"] = producer_files
    a["owning-core-and-callers"] = [
        same(ordinary["core-before"]),
        same(ordinary["executable-before"]),
        same(capi["carrier-before"]),
        same(capi["executable-before"]),
    ]
    retained = json.loads(
        (S / "process-mitigation-retained-owning-candidate2.json").read_text()
    )
    assert retained["state"] == "finished" and len(retained["files"]) == 4
    copies = []
    for row in retained["files"]:
        actual = identity(Path(row["retained"]))
        assert (actual["bytes"], actual["sha256"]) == (row["bytes"], row["sha256"])
        copies.append(actual)
    a["retained-current-artifacts"] = copies
    a["test-image"] = same(gates["test-image-after"])
    a["windows-build"] = sys.getwindowsversion().build
    a.update(
        {
            "state": "verified",
            "exit-code": 0,
            "scope": "Ten DLL inputs (native process-mitigation producer includes MSVCRT; ordinary observers include version.dll), four live current owning artifacts, four separate current retained copies, three native producer sources/twelve executables, and executed native unit image rehashed. The ten DLL records bracket the ordinary executions; no whole-current-root package claim.",
        }
    )
except BaseException:
    a.update({"state": "failed", "exit-code": 1, "error": traceback.format_exc()})
F.write_text(json.dumps(a, indent=2) + "\n")
print(a["state"], flush=True)
raise SystemExit(a["exit-code"])
