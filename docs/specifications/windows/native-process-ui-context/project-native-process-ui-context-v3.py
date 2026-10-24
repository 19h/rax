"""Verify primary query cohorts and emit pointer-free scalar observations."""

from collections import Counter
from pathlib import Path, PureWindowsPath
import hashlib
import json
import re
import shutil

N = Path(__file__).resolve().parent
OUT = N / "process-ui-context-primary-projection-v3"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(raw):
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def verified(label):
    root = N / (label + "-bundle-v1")
    filename = label + ".json" if "cold" in label else "receipt.json"
    record = json.loads((root / filename).read_bytes())
    require(record["state"] == "finished" and record["exit-code"] == 0, "completion")
    for key in ("source", "executable", "runtime"):
        if key + "-before" in record:
            require(
                record[key + "-before"] == record[key + "-after"], key + " stability"
            )
    source = record["source-before"]
    require(
        digest((N / (label + ".cpp")).read_bytes())
        == {k: source[k] for k in ("bytes", "sha256")},
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
            digest(path.read_bytes()) == {k: identity[k] for k in ("bytes", "sha256")},
            "log identity",
        )
    for profile in record.get("architectures", []):
        require(
            profile["executable-before"] == profile["executable-after"],
            "profile executable",
        )
    return root, record, filename


require(not OUT.exists(), "fresh projection")
fixed_root, fixed, fixed_receipt = verified("native-process-ui-context-v2")
profiles = fixed["architectures"]
require(
    len(profiles) == 4
    and {p["profile"] for p in profiles} == {"arm64", "x64", "x86", "x86-laa"},
    "fixed profiles",
)
require(
    len(fixed["steps"]) == 8 and all(p["exit-code"] == 0 for p in fixed["steps"]),
    "fixed native exits",
)
roles = (
    "current",
    "null",
    "thread-pseudo",
    "constant-1234",
    "real-query",
    "real-query-full",
    "real-synchronize",
)
buffers = [("stack", offset) for offset in range(8)] + [
    (name, 0)
    for name in (
        "null",
        "one",
        "readonly",
        "noaccess",
        "guard",
        "cross-readonly",
        "cross-noaccess",
    )
]
expected = Counter(
    (phase, role, buffer, str(offset), str(sample))
    for phase in ("main", "worker")
    for role in roles
    for buffer, offset in buffers
    for sample in range(3)
)
projected = []
for profile in profiles:
    name = profile["profile"]
    raw = (fixed_root / name / ("native-" + name + ".log")).read_bytes()
    lines = raw.decode().splitlines()
    rows = [
        dict(item.split("=", 1) for item in line.split()[1:])
        for line in lines
        if line.startswith("QUERY ")
    ]
    require(len(rows) == 630, "query count")
    require(
        Counter(
            tuple(r[k] for k in ("phase", "role", "buffer", "offset", "sample"))
            for r in rows
        )
        == expected,
        "query coverage",
    )
    require(
        [line for line in lines if line.startswith("OPEN role=real-zero ")]
        == ["OPEN role=real-zero access=00000000 present=0 error=00000005"] * 2,
        "unavailable zero grant",
    )
    transitions = Counter()
    for row in rows:
        valid = row["role"] in ("current", "real-query", "real-query-full")
        success = valid and row["buffer"] == "stack"
        guard = valid and row["buffer"] == "guard"
        error = (
            "12345678"
            if success
            else ("80000001" if guard else ("000003E6" if valid else "00000057"))
        )
        require(
            row["result"] == str(int(success)) and row["error"] == error,
            "query return/error",
        )
        require(
            row["stack-changed"] == ("8" if success else "0"), "stack output extent"
        )
        if row["readable"] == "1":
            require(
                row["data"] == ("0000000000000000" if success else "A5A5A5A5A5A5A5A5"),
                "query output",
            )
        else:
            require(row["data"] == "", "unreadable output disclosure")
        if row["buffer"] == "guard":
            require(
                row["protect"] == ("00000004" if valid else "00000104"),
                "guard priority/consumption",
            )
        if name.startswith("x86") or success or not valid:
            require(row["nt-before"] == row["nt-after"], "preserved native status")
        else:
            require(
                row["nt-after"] == ("80000001" if guard else "C0000005"),
                "fault native status",
            )
        transitions[(row["nt-before"], row["nt-after"])] += 1
    projected.append(
        {
            "profile": name,
            "queries": len(rows),
            "native64-status-measured": not name.startswith("x86"),
            "status-transitions": [
                {"before": a, "after": b, "count": count}
                for (a, b), count in sorted(transitions.items())
            ],
            "private-log": digest(raw),
        }
    )

cold_controls = []
cold_inputs = []
for version in (1, 2):
    label = "native-process-ui-context-cold-v" + str(version)
    root, record, filename = verified(label)
    cold_inputs.append((label, root, filename))
    cases = (
        [
            ("normal", 0, 0, 1, "12345678", 1),
            ("callback-success", 0, 0, 1, "12345678", 1),
            ("callback-failure", 5, 0xC0000001, 0, "12345678", 0),
            ("output-invalid", 0, 0, 1, "12345678", 1),
            ("output-warning", 5, 0x80000001, 0, "12345678", 0),
        ]
        if version == 1
        else [
            (mode, 0, 0, 0, "000003E6" if mode == "query-null" else "00000057", 1)
            for mode in (
                "query-null",
                "query-thread",
                "query-thread-null",
                "query-zero",
            )
        ]
    )
    require(len(record["steps"]) == (10 if version == 1 else 5), "cold step coverage")
    outcomes = {row["mode"]: row for row in record["native-outcomes"]}
    for mode, exit_code, status, value, error, marker in cases:
        raw = (root / (label + "-" + mode + ".log")).read_bytes()
        text = raw.decode().replace("\r\n", "\n")
        require(
            re.findall(
                r"cold-query-before win32-thread-present=(\d+) callbacks=(\d+)", text
            )
            == [("0", "0")],
            "cold initial state",
        )
        require(
            re.findall(
                r"cold-query-return value=(\d+) last_error=([0-9A-F]+) win32-thread-present=(\d+) callbacks=(\d+)",
                text,
            )
            == [(str(value), error, str(marker), "1")],
            "cold result/error/marker",
        )
        entries = re.findall(
            r"^callback=\d+ init-events=(\d+) index=(\d+) length=(\d+)", text, re.M
        )
        require(entries and entries[0] == ("0", "84", "0"), "cold callback")
        returns = re.findall(
            r"^callback-return=\d+ init-events=(\d+) buffer=([0-9a-f]+) length=(\d+) status=([0-9a-f]+)",
            text,
            re.M | re.I,
        )
        require(
            returns and returns[0][0] == "0" and int(returns[0][3], 16) == status,
            "cold callback status",
        )
        if mode in ("output-invalid", "output-warning"):
            require(
                int(returns[0][1], 16) == 1 and int(returns[0][2]) == 0xFFFFFFFF,
                "invalid callback output",
            )
        expected_output = (
            "00000000,00000000,A5A5A5A5,A5A5A5A5"
            if value
            else "A5A5A5A5,A5A5A5A5,A5A5A5A5,A5A5A5A5"
        )
        require(
            re.findall(r"^cold-ui-output=(.*)$", text, re.M)
            == [expected_output] * (2 if mode == "normal" or version == 2 else 1),
            "cold output extent",
        )
        outcome = outcomes[mode]
        require(
            outcome["exit-code"] == exit_code and outcome["before"] == outcome["after"],
            "cold native outcome",
        )
        cold_controls.append(
            {
                "producer": version,
                "mode": mode,
                "native-exit-code": exit_code,
                "callback-index": 84,
                "callback-input-bytes": 0,
                "callback-status": f"0x{status:08X}",
                "return": value,
                "last-error": error,
                "conversion-marker-after": bool(marker),
                "private-log": digest(raw),
            }
        )
    if version == 1:
        for mode in ("query-null", "query-thread", "query-thread-null", "query-zero"):
            require(outcomes[mode]["exit-code"] == 10, "rejected producer mode")
            require(
                (root / (label + "-" + mode + ".log")).read_bytes() == b"",
                "rejected mode did not run query",
            )

kernel = json.loads(
    (N / "remote-connect-primary-projection-v3/kernel-inputs.json").read_bytes()
)
current = json.loads((N / "lpk-register-win32k-current-v1.json").read_bytes())
require(
    current == {k: kernel["image"][k] for k in ("bytes", "sha256")},
    "selected kernel identity",
)
kernel["functions"] = [
    {
        "name": "NtUserGetProcessUIContextInformation",
        "section": 1,
        "section-rva": "0x1000",
        "section-offset": "0x8DCC0",
        "rva": "0x8ECC0",
    }
]
kernel.pop("static-result-domain")
kernel.pop("native-observed-result-domain")
kernel["static-output"] = {
    "bytes": 8,
    "DWORD-fields": 2,
    "field-domains": [[0, 3], [0, 3]],
    "semantic-names": "unknown",
    "process-Win32-offset": "0x320",
    "native-current-values": [0, 0],
}
OUT.mkdir()
for label, root, filename in [
    ("native-process-ui-context-v2", fixed_root, fixed_receipt),
    *cold_inputs,
]:
    for suffix in (".cpp",):
        shutil.copyfile(N / (label + suffix), OUT / (label + suffix))
    shutil.copyfile(N / ("run-" + label + ".py"), OUT / ("run-" + label + ".py"))
    shutil.copyfile(root / filename, OUT / (label + "-receipt.json"))
for name in (
    "native-process-ui-context-kernel-code-v1.txt",
    "process-ui-context-primary-readme-v1.md",
):
    shutil.copyfile(N / name, OUT / ("README.md" if name.endswith(".md") else name))
(OUT / "kernel-inputs.json").write_text(json.dumps(kernel, indent=2) + "\n")
(OUT / "observations.json").write_text(
    json.dumps(
        {
            "scope": "Four executable views share one ARM64 kernel and SYSTEM context. Raw identifiers and memory excluded. Native field semantics and independent x64 kernel remain unknown.",
            "fixed-query-count": 2520,
            "profiles": projected,
            "cold-controls": cold_controls,
            "cold-v1-rejected-modes": 4,
            "DPI-controls": "Twelve setters rejected; twelve manifest variants all reported effective DPI2. No inference about field names or DPI independence.",
        },
        indent=2,
    )
    + "\n"
)
(OUT / ".gitattributes").write_text("* -text\n")
shutil.copyfile(Path(__file__), OUT / Path(__file__).name)
files = [{"path": p.name, **digest(p.read_bytes())} for p in sorted(OUT.iterdir())]
(OUT / "inputs.json").write_text(json.dumps({"files": files}, indent=2) + "\n")
print("projected2520 queries and9 accepted cold controls;4 rejected modes excluded")
