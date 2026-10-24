"""Publish scalar projections; retain native identifiers and memory privately."""

from pathlib import Path, PureWindowsPath
from collections import Counter
import hashlib
import json
import re
import shutil

N = Path(__file__).resolve().parent
OUT = N / "thread-desktop-primary-projection-v2"


def require(value, message):
    if not value:
        raise ValueError(message)


def digest(raw):
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def verified(root, receipt_name, source_name):
    record = json.loads((root / receipt_name).read_bytes())
    require(record["state"] == "finished" and record["exit-code"] == 0, "completion")
    for key in ("source", "executable", "runtime", "native-modules"):
        if key + "-before" in record:
            require(
                record[key + "-before"] == record[key + "-after"], key + " stability"
            )
    source = record["source-before"]
    require(
        digest((N / source_name).read_bytes())
        == {key: source[key] for key in ("bytes", "sha256")},
        "source identity",
    )
    origin = PureWindowsPath(source.get("path", source.get("file"))).parent
    for step in record["steps"]:
        identity = step["log"]
        relative = PureWindowsPath(
            identity.get("path", identity.get("file"))
        ).relative_to(origin)
        path = root.joinpath(*relative.parts)
        require(path.resolve().is_relative_to(root.resolve()), "log containment")
        require(
            digest(path.read_bytes())
            == {key: identity[key] for key in ("bytes", "sha256")},
            "log identity",
        )
    for arch in record.get("architectures", []):
        require(
            arch["executable-before"] == arch["executable-after"], "profile executable"
        )
    return record


require(not OUT.exists(), "fresh projection")
roots = {}
receipts = {}
for version in ("v3", "v4"):
    label = "native-thread-desktop-" + version
    root = N / (label + "-bundle-v1")
    roots[version] = root
    receipts[version] = verified(root, "receipt.json", label + ".cpp")
    require(
        len(receipts[version]["steps"]) == 20, "four build and sixteen runtime steps"
    )
    require(
        all(row["exit-code"] == 0 for row in receipts[version]["steps"]),
        "native profile exits",
    )
coldroot = N / "native-thread-desktop-cold-v1-bundle-v1"
cold = verified(
    coldroot, "native-thread-desktop-cold-v1.json", "native-thread-desktop-cold-v1.cpp"
)
profiles = []
for arch in receipts["v3"]["architectures"]:
    profile = arch["profile"]
    counts = Counter()
    logs = []
    worker_rows = []
    for mode in ("query", "close-desktop", "nt-close", "dup-close-source"):
        relative = profile + "/native-" + profile + "-" + mode + ".log"
        path = roots["v3"] / relative
        raw = path.read_bytes()
        text = raw.decode()
        rows = re.findall(
            r"^QUERY phase=(\S+) role=(\S+) sample=(\d+) id=[0-9A-F]+ present=([01]) same-current=([01]) value=[0-9A-Fa-f]+ error=([0-9A-F]+) nt-before=([0-9A-F]+) nt-after=([0-9A-F]+)\r?$",
            text,
            re.M,
        )
        require(
            len(rows) == sum(line.startswith("QUERY ") for line in text.splitlines()),
            "query grammar",
        )
        for phase, role, sample, present, same, error, before, after in rows:
            require(
                error == "12345678" and before == after,
                "thread query error preservation",
            )
            require(present == same, "borrowed identity")
            counts[(phase, role, bool(int(present)))] += 1
        require("NAME label=current-final ok=1" in text, "assigned handle survives")
        basics = re.findall(
            r"BASIC label=\S+ status=([0-9A-F]+) error=([0-9A-F]+) returned=(\d+) attrs=([0-9A-F]+) access=([0-9A-F]+)",
            text,
        )
        require(
            basics
            and set(basics) == {("00000000", "12345678", "56", "00000000", "000F00CF")},
            "desktop access metadata",
        )
        flags = re.findall(
            r"HANDLE_FLAGS label=\S+ ok=(\d+) error=([0-9A-F]+) flags=([0-9A-F]+)", text
        )
        require(
            flags and set(flags) == {("1", "12345678", "00000000")},
            "desktop handle flags",
        )
        if mode == "close-desktop":
            require(
                "CLOSE_CURRENT ok=0 error=000000AA" in text,
                "CloseDesktop assigned denial",
            )
        elif mode == "nt-close":
            require(
                "NT_CLOSE_CURRENT status=C0000235 error=12345678" in text,
                "NtClose assigned denial",
            )
        elif mode == "dup-close-source":
            require(
                "DUP_CLOSE_CURRENT ok=1 copy-present=1 error=12345678" in text,
                "duplicate close-source result",
            )
        logs.append({"path": relative, **digest(raw)})
        worker_text = (roots["v4"] / relative).read_text()
        worker_rows += re.findall(
            r"WORKER_SELF_QUERY present=(\d+) error=([0-9A-F]+) gui-before=(\d+) gui-after=(\d+)",
            worker_text,
        )
    require(sum(counts.values()) == 261, "profile query count")
    expected_marker = "1" if profile in ("arm64", "x64") else "0"
    require(
        len(worker_rows) == 8
        and set(worker_rows) == {("1", "12345678", "0", expected_marker)},
        "worker caller conversion",
    )
    profiles.append(
        {
            "profile": profile,
            "query-rows": 261,
            "groups": [
                {
                    "phase": key[0],
                    "role": key[1],
                    "present-and-same-desktop": key[2],
                    "rows": count,
                }
                for key, count in sorted(counts.items())
            ],
            "last-error-preserved": True,
            "last-nt-status-preserved": True,
            "access": "0x000F00CF",
            "handle-flags": 0,
            "assigned-CloseDesktop": {"result": False, "error": "0x000000AA"},
            "assigned-NtClose": "0xC0000235",
            "duplicate-close-source": {
                "result": True,
                "duplicate-present": True,
                "original-valid-after": True,
            },
            "worker-self-query": {
                "rows": 8,
                "marker-before": False,
                "marker-after": expected_marker == "1",
                "marker-view": "TEB32" if profile.startswith("x86") else "native64",
            },
            "private-query-logs": logs,
        }
    )
controls = []
for mode, exit_code, status, present in (
    ("normal", 0, 0, True),
    ("callback-success", 0, 0, True),
    ("callback-failure", 5, 0xC0000001, False),
    ("output-invalid", 0, 0, True),
    ("output-warning", 5, 0x80000001, False),
):
    raw = (coldroot / ("native-thread-desktop-cold-v1-" + mode + ".log")).read_bytes()
    text = raw.decode()
    require(
        re.findall(
            r"cold-query-before win32-thread-present=(\d+) callbacks=(\d+)", text
        )
        == [("0", "0")],
        "cold initial state",
    )
    rows = re.findall(
        r"cold-query-return value=(\d+) last_error=([0-9A-F]+) win32-thread-present=(\d+) callbacks=(\d+)",
        text,
    )
    require(len(rows) == 1, "one cold return")
    value, error, marker, callbacks = rows[0]
    require(
        (int(value) != 0) == present
        and error == "12345678"
        and bool(int(marker)) == present
        and callbacks == "1",
        "cold result",
    )
    entries = re.findall(
        r"^callback=\d+ init-events=(\d+) index=(\d+) length=(\d+)", text, re.M
    )
    require(entries and entries[0] == ("0", "84", "0"), "first callback")
    returns = re.findall(
        r"^callback-return=\d+ init-events=(\d+) buffer=([0-9a-f]+) length=(\d+) status=([0-9a-f]+)",
        text,
        re.M | re.I,
    )
    require(returns and int(returns[0][3], 16) == status, "first callback status")
    if mode == "output-invalid":
        require(
            int(returns[0][1], 16) == 1 and int(returns[0][2]) == 0xFFFFFFFF,
            "invalid output control",
        )
    outcome = next(row for row in cold["native-outcomes"] if row["mode"] == mode)
    require(
        outcome["exit-code"] == exit_code and outcome["before"] == outcome["after"],
        "native controlled exit",
    )
    controls.append(
        {
            "mode": mode,
            "native-exit-code": exit_code,
            "callback-index": 84,
            "callback-input-bytes": 0,
            "callback-status": f"0x{status:08X}",
            "return-nonzero": present,
            "conversion-marker-after": present,
            "last-error": "0x12345678",
            "private-log": digest(raw),
        }
    )
OUT.mkdir()
for version in ("v3", "v4", "cold-v1"):
    name = "native-thread-desktop-" + version
    shutil.copyfile(N / (name + ".cpp"), OUT / (name + ".cpp"))
    shutil.copyfile(N / ("run-" + name + ".py"), OUT / ("run-" + name + ".py"))
for version, root in roots.items():
    shutil.copyfile(root / "receipt.json", OUT / (version + "-receipt.json"))
shutil.copyfile(
    coldroot / "native-thread-desktop-cold-v1.json", OUT / "cold-receipt.json"
)
(OUT / "observations.json").write_text(
    json.dumps(
        {
            "scope": "Four executable views on one ARM64 Windows kernel, SYSTEM context. Native x64-kernel and native64 WoW64 marker effects remain unknown. No native IDs, handle values or private memory are projected.",
            "cid-retirement-limitation": "Last user handle close is not proof of immediate native CID deletion; delayed object references or TID reuse are unresolved.",
            "profiles": profiles,
            "cold-controls": controls,
        },
        indent=2,
    )
    + "\n"
)
(OUT / ".gitattributes").write_text("* -text\n")
shutil.copyfile(Path(__file__), OUT / Path(__file__).name)
files = [
    {"path": p.relative_to(OUT).as_posix(), **digest(p.read_bytes())}
    for p in sorted(OUT.rglob("*"))
    if p.is_file()
]
(OUT / "inputs.json").write_text(json.dumps({"files": files}, indent=2) + "\n")
print("projected", len(profiles), "profiles", len(controls), "cold controls")
