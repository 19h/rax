"""Publish scalar projections; retain native identifiers and memory privately."""

from pathlib import Path, PureWindowsPath
from collections import Counter
import hashlib
import json
import re
import shutil

N = Path(__file__).resolve().parent
OUT = N / "lpk-registration-primary-projection-v1"


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
root = N / "native-lpk-register-v1-bundle-v1"
fixed = verified(root, "receipt.json", "native-lpk-register-v1.cpp")
coldroot = N / "native-lpk-register-cold-v1-bundle-v1"
cold = verified(
    coldroot, "native-lpk-register-cold-v1.json", "native-lpk-register-cold-v1.cpp"
)
arguments = [
    0,
    1,
    2,
    3,
    0x10,
    0x100,
    0x10000,
    0x10000000,
    0x40000000,
    0x7FFFFFFF,
    0x80000000,
    0x80000001,
    0xFFFFFFFE,
    0xFFFFFFFF,
    0x13572468,
    0,
]
profiles = []
require(
    {row["profile"] for row in fixed["architectures"]}
    == {"arm64", "x64", "x86", "x86-laa"}
    and len(fixed["architectures"]) == 4,
    "fixed profile coverage",
)
require(
    len(fixed["steps"]) == 8 and all(row["exit-code"] == 0 for row in fixed["steps"]),
    "fixed native exits",
)
for arch in fixed["architectures"]:
    profile = arch["profile"]
    path = root / profile / ("native-" + profile + ".log")
    raw = path.read_bytes()
    text = raw.decode()
    rows = re.findall(
        r"REGISTER phase=(\S+) sample=(\d+) argument=([0-9A-F]+) value=(\d+) last_error=([0-9A-F]+) nt-before=([0-9A-F]+) nt-after=([0-9A-F]+)",
        text,
    )
    require(len(rows) == 96, "registration count")
    expected = Counter(
        (phase, str(sample), value)
        for phase in ("main", "worker")
        for sample in range(3)
        for value in arguments
    )
    require(
        Counter(
            (phase, sample, int(argument, 16)) for phase, sample, argument, *_ in rows
        )
        == expected,
        "registration argument coverage",
    )
    require(
        all(
            value == "1" and error == "12345678" and before == after
            for _, _, _, value, error, before, after in rows
        ),
        "registration result and errors",
    )
    stubs = re.findall(r"STUB phase=(\S+) bytes=([0-9A-F]+)", text)
    require(
        len(stubs) == 2 and {phase for phase, _ in stubs} == {"main", "worker"},
        "stub coverage",
    )
    require(stubs[0][1] == stubs[1][1], "same selected leaf")
    code = bytes.fromhex(stubs[0][1])
    require(len(code) == 32, "stub extent")
    if profile.startswith("x86"):
        require(code[12:15] == bytes.fromhex("C20400"), "DWORD x86 cleanup")
    profiles.append(
        {
            "profile": profile,
            "calls": 96,
            "arguments": [f"0x{x:08X}" for x in arguments],
            "return-value": 1,
            "last-error": "0x12345678",
            "last-nt-status-preserved": True,
            "private-log": digest(raw),
            "private-leaf": digest(code),
            "x86-stack-cleanup-bytes": 4 if profile.startswith("x86") else None,
        }
    )
controls = []
for mode, exit_code, status, value in (
    ("normal", 0, 0, 1),
    ("callback-success", 0, 0, 1),
    ("callback-failure", 5, 0xC0000001, 0),
    ("output-invalid", 0, 0, 1),
    ("output-warning", 5, 0x80000001, 0),
):
    raw = (coldroot / ("native-lpk-register-cold-v1-" + mode + ".log")).read_bytes()
    text = raw.decode()
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
        == [(str(value), "12345678", str(value), "1")],
        "cold result and errors",
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
    outcome = next(row for row in cold["native-outcomes"] if row["mode"] == mode)
    require(
        outcome["exit-code"] == exit_code and outcome["before"] == outcome["after"],
        "controlled native exit",
    )
    controls.append(
        {
            "mode": mode,
            "argument": "0x13572468",
            "callback-index": 84,
            "callback-input-bytes": 0,
            "callback-status": f"0x{status:08X}",
            "return-value": value,
            "conversion-marker-after": bool(value),
            "last-error": "0x12345678",
            "native-exit-code": exit_code,
            "private-log": digest(raw),
        }
    )
kernel = json.loads(
    (N / "remote-connect-primary-projection-v3/kernel-inputs.json").read_bytes()
)
current = json.loads((N / "lpk-register-win32k-current-v1.json").read_bytes())
imports = json.loads((N / "native-lpk-kernel-imports-v1.json").read_bytes())
require(
    current
    == imports["input"]
    == {key: kernel["image"][key] for key in ("bytes", "sha256")},
    "selected kernel identity",
)
kernel["functions"] = [
    {
        "name": "NtUserRegisterLPK",
        "section": 1,
        "section-rva": "0x1000",
        "section-offset": "0x94A80",
        "rva": "0x95A80",
    }
]
kernel.pop("static-result-domain")
kernel.pop("native-observed-result-domain")
kernel["selected-function-effect"] = {
    "DWORD-store-offset-from-process-Win32-state": "0x2F0",
    "return": 1,
    "native-kernel-memory-field-read": False,
    "initial-value": "unknown",
}
OUT.mkdir()
for name in (
    "native-lpk-register-v1.cpp",
    "run-native-lpk-register-v1.py",
    "native-lpk-register-cold-v1.cpp",
    "run-native-lpk-register-cold-v1.py",
    "native-lpk-kernel-imports-v1.json",
    "native-lpk-kernel-code-v1.txt",
):
    shutil.copyfile(N / name, OUT / name)
shutil.copyfile(root / "receipt.json", OUT / "fixed-receipt.json")
shutil.copyfile(
    coldroot / "native-lpk-register-cold-v1.json", OUT / "cold-receipt.json"
)
shutil.copyfile(N / "lpk-registration-primary-readme-v1.md", OUT / "README.md")
(OUT / "kernel-inputs.json").write_text(json.dumps(kernel, indent=2) + "\n")
(OUT / "observations.json").write_text(
    json.dumps(
        {
            "scope": "Four executable views of one ARM64 Windows kernel. Native initial field state and independent x64 kernel remain unknown. Raw identifiers and private memory excluded.",
            "fixed-call-count": 384,
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
    {"path": path.name, **digest(path.read_bytes())} for path in sorted(OUT.iterdir())
]
(OUT / "inputs.json").write_text(json.dumps({"files": files}, indent=2) + "\n")
print("projected384 fixed calls and5 cold controls")
