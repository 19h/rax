"""Project fixed query controls without publishing private memory captures."""

from pathlib import Path, PureWindowsPath
import hashlib
import json
import re
import shutil

N = Path(__file__).resolve().parent
OUT = N / "remote-connect-primary-projection-v3"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(raw):
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def load(path):
    return json.loads(path.read_bytes())


def verify_record(root, name, source):
    record = load(root / name)
    require(
        record["state"] == "finished" and record["exit-code"] == 0,
        "producer completion",
    )
    for before, after in [
        ("source-before", "source-after"),
        ("executable-before", "executable-after"),
        ("runtime-before", "runtime-after"),
        ("native-modules-before", "native-modules-after"),
    ]:
        if before in record:
            require(record[before] == record[after], before + " stability")
    identity = record["source-before"]
    require(
        digest((N / source).read_bytes())
        == {k: identity[k] for k in ("bytes", "sha256")},
        "source identity",
    )
    for row in record["steps"]:
        identity = row["log"]
        path = identity.get("file", identity.get("path"))
        name = path.replace("\\", "/").split("/")[-1]
        origin = PureWindowsPath(
            record["source-before"].get("file", record["source-before"].get("path"))
        ).parent
        relative = PureWindowsPath(path).relative_to(origin)
        candidate = root.joinpath(*relative.parts)
        require(candidate.resolve().is_relative_to(root.resolve()), "log containment")
        require(
            digest(candidate.read_bytes())
            == {k: identity[k] for k in ("bytes", "sha256")},
            "log identity " + name,
        )
    return record


require(not OUT.exists(), "fresh output")
root = N / "native-remote-connect-state-v2-bundle-v1"
fixed = verify_record(root, "receipt.json", "native-remote-connect-state-v2.cpp")
coldroot = N / "native-remote-cold-bundle-v1"
cold = verify_record(
    coldroot, "native-remote-connect-cold-v1.json", "native-remote-connect-cold-v1.cpp"
)
profiles = []
for arch in fixed["architectures"]:
    require(arch["executable-before"] == arch["executable-after"], "profile executable")
    profile = arch["profile"]
    path = root / profile / ("native-" + profile + ".log")
    raw = path.read_bytes()
    rows = re.findall(
        r"QUERY phase=(\w+) stage=(\d+) sample=(\d+) value=(\d+) last_error=([0-9A-F]+)",
        raw.decode(),
    )
    expected = {
        (p, str(t), str(i))
        for p in ("main", "worker")
        for t in range(2)
        for i in range(32)
    }
    require(
        len(rows) == 128 and {(p, t, i) for p, t, i, _, _ in rows} == expected,
        "query coverage",
    )
    require({(v, e) for _, _, _, v, e in rows} == {("0", "12345678")}, "query values")
    profiles.append(
        {
            "profile": profile,
            "rows": 128,
            "value": 0,
            "last-error": "0x12345678",
            "log": {"path": path.relative_to(root).as_posix(), **digest(raw)},
        }
    )
controls = []
for mode, code, status, marker in [
    ("normal", 0, 0, 1),
    ("callback-success", 0, 0, 1),
    ("callback-failure", 5, 0xC0000001, 0),
    ("output-warning", 5, 0x80000001, 0),
]:
    path = coldroot / ("native-remote-connect-cold-v1-" + mode + ".log")
    raw = path.read_bytes()
    text = raw.decode()
    before = re.findall(
        r"cold-query-before win32-thread-present=(\d+) callbacks=(\d+)", text
    )
    after = re.findall(
        r"cold-query-return value=(\d+) last_error=([0-9A-F]+) win32-thread-present=(\d+) callbacks=(\d+)",
        text,
    )
    callbacks = re.findall(
        r"^callback=\d+ init-events=(\d+) index=(\d+) length=(\d+)", text, re.M
    )
    returns = re.findall(
        r"^callback-return=\d+ init-events=(\d+) buffer=[0-9a-f]+ length=(\d+) status=([0-9a-f]+)",
        text,
        re.M | re.I,
    )
    require(before == [("0", "0")], "cold marker before")
    require(after == [("0", "12345678", str(marker), "1")], "cold query after")
    require(callbacks and callbacks[0] == ("0", "84", "0"), "cold callback selection")
    require(
        returns and int(returns[0][2], 16) == status and returns[0][0] == "0",
        "cold callback return",
    )
    outcome = next(row for row in cold["native-outcomes"] if row["mode"] == mode)
    require(
        outcome["exit-code"] == code and outcome["before"] == outcome["after"],
        "controlled native exit",
    )
    controls.append(
        {
            "mode": mode,
            "native-exit-code": code,
            "cold-before": {"win32-thread-present": False, "callbacks": 0},
            "callback": {"index": 84, "input-bytes": 0, "status": f"0x{status:08X}"},
            "query-return": {
                "value": 0,
                "last-error": "0x12345678",
                "win32-thread-present": bool(marker),
                "callbacks": 1,
            },
            "original-private-log": digest(raw),
        }
    )
outputroot = N / "native-remote-cold-output-bundle-v1"
output = load(outputroot / "receipt.json")
require(
    output["state"] == "finished"
    and output["exit-code"] == 0
    and output["native-exit-code"] == 0,
    "output control completion",
)
for key in ("source", "executable", "runtime"):
    require(
        output[key + "-before"] == output[key + "-after"] == cold[key + "-before"],
        "output control " + key,
    )
require(
    {key: output["producer-receipt"][key] for key in ("bytes", "sha256")}
    == digest((coldroot / "native-remote-connect-cold-v1.json").read_bytes()),
    "output control producer binding",
)
raw = (outputroot / "native-output-invalid.log").read_bytes()
require(
    digest(raw) == {key: output["log"][key] for key in ("bytes", "sha256")},
    "output control log identity",
)
text = raw.decode()
require(
    re.findall(r"cold-query-before win32-thread-present=(\d+) callbacks=(\d+)", text)
    == [("0", "0")],
    "output control before",
)
require(
    re.findall(
        r"cold-query-return value=(\d+) last_error=([0-9A-F]+) win32-thread-present=(\d+) callbacks=(\d+)",
        text,
    )
    == [("0", "12345678", "1", "1")],
    "output control after",
)
entries = re.findall(
    r"^callback=\d+ init-events=(\d+) index=(\d+) length=(\d+)", text, re.M
)
require(entries and entries[0] == ("0", "84", "0"), "output control callback")
returns = re.findall(
    r"^callback-return=\d+ init-events=(\d+) buffer=([0-9a-f]+) length=(\d+) status=([0-9a-f]+)",
    text,
    re.M | re.I,
)
require(
    returns
    and returns[0][0] == "0"
    and int(returns[0][1], 16) == 1
    and int(returns[0][2]) == 0xFFFFFFFF
    and int(returns[0][3], 16) == 0,
    "output control invalid output and successful status",
)
controls.append(
    {
        "mode": "output-invalid",
        "native-exit-code": 0,
        "cold-before": {"win32-thread-present": False, "callbacks": 0},
        "callback": {
            "index": 84,
            "input-bytes": 0,
            "status": "0x00000000",
            "output-buffer-is-1": True,
            "output-bytes": 0xFFFFFFFF,
        },
        "query-return": {
            "value": 0,
            "last-error": "0x12345678",
            "win32-thread-present": True,
            "callbacks": 1,
        },
        "original-private-log": digest(raw),
    }
)
OUT.mkdir()
for name in [
    "native-remote-connect-state-v2.cpp",
    "run-native-remote-connect-state-v2.py",
    "native-remote-connect-cold-v1.cpp",
    "run-native-remote-connect-cold-v1.py",
    "run-native-remote-connect-cold-output-v2.py",
]:
    shutil.copyfile(N / name, OUT / name)
shutil.copyfile(root / "receipt.json", OUT / "fixed-receipt.json")
shutil.copyfile(
    coldroot / "native-remote-connect-cold-v1.json", OUT / "cold-receipt.json"
)
shutil.copyfile(outputroot / "receipt.json", OUT / "output-invalid-receipt.json")
for arch in fixed["architectures"]:
    profile = arch["profile"]
    shutil.copyfile(
        root / profile / ("native-" + profile + ".log"),
        OUT / ("fixed-" + profile + ".log"),
    )
for src, name in [
    (root / "arm64/build-arm64.log", "fixed-build-arm64.log"),
    (coldroot / "native-remote-connect-cold-v1-build.log", "cold-build.log"),
]:
    shutil.copyfile(src, OUT / name)
result = {
    "scope": "Four execution views of one Windows ARM64 10.0.29683 kernel; enum0 native controls, cold conversion and controlled callback failure. Independent x64 kernel and nonzero native session states remain unknown.",
    "fixed-query-count": sum(x["rows"] for x in profiles),
    "fixed-profiles": profiles,
    "cold-controls": controls,
    "cold-private-log-policy": "Only relative/scalar fields are projected; original private memory logs are bound by producer receipts and original hashes.",
    "whole-goal-complete": False,
}
(OUT / "observations.json").write_text(json.dumps(result, indent=2) + "\n")
(OUT / ".gitattributes").write_text("* -text\n")
shutil.copyfile(Path(__file__), OUT / Path(__file__).name)
for name in ("README.md", "kernel-inputs.json"):
    shutil.copyfile(N / "remote-connect-v3-material" / name, OUT / name)
rows = []
for p in sorted(OUT.iterdir()):
    rows.append({"path": p.name, **digest(p.read_bytes())})
(OUT / "inputs.json").write_text(json.dumps({"files": rows}, indent=2) + "\n")
print(
    "projected",
    len(profiles),
    "profiles",
    len(controls),
    "cold controls; private logs excluded",
)
