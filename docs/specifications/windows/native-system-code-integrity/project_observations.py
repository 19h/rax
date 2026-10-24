"""Verify the complete native cohorts and publish only controlled scalar fields."""

from collections import Counter
from pathlib import Path, PureWindowsPath
import argparse
import hashlib
import itertools
import json
import re
import shutil


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(raw):
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def identity(row):
    return {key: row[key] for key in ("bytes", "sha256")}


def coverage(version):
    if version == 1:
        first = [
            ("stack", str(offset), str(length), f"{embedded:08X}", "valid")
            for offset, length, embedded in itertools.product(
                (0, 1, 3, 4, 7), (0, 1, 4, 7, 8, 9, 16), (0, 4, 8, 16, 0xFFFFFFFF)
            )
        ]
        second = [
            (buffer, "0", str(length), f"{embedded:08X}", returned)
            for buffer, length, embedded, returned in itertools.product(
                (
                    "stack",
                    "null",
                    "one",
                    "readonly",
                    "noaccess",
                    "guard",
                    "cross-readonly",
                    "cross-noaccess",
                ),
                (0, 7, 8, 16),
                (0, 8),
                (
                    "valid",
                    "null",
                    "one",
                    "readonly",
                    "guard",
                    "unaligned",
                    "alias-length",
                    "alias-options",
                ),
            )
        ]
        cases = first + second
    else:
        cases = [
            (buffer, "0", str(length), "00000008", returned)
            for buffer, length, returned in itertools.product(
                (
                    "stack",
                    "readonly-guard",
                    "guard",
                    "cross-readonly",
                    "cross-noaccess",
                ),
                (0, 8, 16, 0xFFFFFFFF),
                ("valid", "null", "guard", "readonly-guard"),
            )
        ]
    return Counter((str(sample), *case) for sample in range(3) for case in cases)


def project(root, version):
    label = f"native-system-code-integrity-v{version}"
    bundle = root / (label + "-bundle-v1")
    receipt_raw = (bundle / "receipt.json").read_bytes()
    receipt = json.loads(receipt_raw)
    require(receipt["state"] == "finished" and receipt["exit-code"] == 0, "completion")
    for key in ("source", "native-modules", "identity"):
        require(receipt[key + "-before"] == receipt[key + "-after"], key + " stability")
    source = bundle / (label + ".cpp")
    require(
        digest(source.read_bytes()) == identity(receipt["source-before"]),
        "source identity",
    )
    profiles = receipt["architectures"]
    require(
        len(profiles) == 4
        and {p["profile"] for p in profiles} == {"arm64", "x64", "x86", "x86-laa"},
        "profiles",
    )
    require(
        len(receipt["steps"]) == 8
        and all(s["exit-code"] == 0 for s in receipt["steps"]),
        "native steps",
    )
    origin = PureWindowsPath(receipt["source-before"]["path"]).parent
    logs = []
    for step in receipt["steps"]:
        rel = PureWindowsPath(step["log"]["path"]).relative_to(origin)
        path = bundle.joinpath(*rel.parts)
        require(path.resolve().is_relative_to(bundle.resolve()), "log containment")
        require(digest(path.read_bytes()) == identity(step["log"]), "log identity")
        logs.append({"path": rel.as_posix(), **identity(step["log"])})
    expected = coverage(version)
    projected = []
    for profile in profiles:
        name = profile["profile"]
        require(
            profile["executable-before"] == profile["executable-after"],
            "executable stability",
        )
        lines = (bundle / name / ("native-" + name + ".log")).read_text().splitlines()
        rows = [
            dict(item.split("=", 1) for item in line.split()[1:])
            for line in lines
            if line.startswith("QUERY ")
        ]
        require(
            Counter(
                tuple(
                    r[k]
                    for k in (
                        "sample",
                        "buffer",
                        "offset",
                        "length",
                        "embedded",
                        "returned",
                    )
                )
                for r in rows
            )
            == expected,
            "complete case multiplicity",
        )
        for row in rows:
            require(
                row["status"]
                in {"00000000", "C0000004", "C0000005", "80000001", "80000002"},
                "status domain",
            )
            require(
                row["error"] == "12345678" and row["nt-before"] == row["nt-after"],
                "preserved errors",
            )
            require(
                row["readable"] in {"0", "1"} and row["return-readable"] in {"0", "1"},
                "readability domain",
            )
            item = {
                key: row[key]
                for key in (
                    "buffer",
                    "embedded",
                    "returned",
                    "status",
                    "error",
                    "output-protect",
                    "return-protect",
                    "return-value",
                )
            }
            item.update(
                profile=name,
                sample=int(row["sample"]),
                offset=int(row["offset"]),
                length=int(row["length"]),
                readable=row["readable"] == "1",
                **{
                    "return-readable": row["return-readable"] == "1",
                    "last-status-preserved": True,
                },
            )
            if item["readable"]:
                require(
                    re.fullmatch(r"[0-9A-F]{32}", row["data"]) is not None,
                    "controlled output length",
                )
                raw = bytes.fromhex(row["data"])
                require(raw[8:] == b"\xa5" * 8, "output tail")
                item.update(
                    {
                        "output-length": int.from_bytes(raw[:4], "little"),
                        "output-options": f"{int.from_bytes(raw[4:8], 'little'):08X}",
                        "output-tail-preserved": True,
                    }
                )
            else:
                require(row["data"] == "", "unreadable output")
            projected.append(item)
    provenance = {
        "cohort": label,
        "receipt": digest(receipt_raw),
        "source": {"path": source.name, **digest(source.read_bytes())},
        "private-log-identities": logs,
        "stable-source-runtime-identity-executables": True,
        "executables": [
            {"profile": p["profile"], **identity(p["executable-before"])}
            for p in profiles
        ],
        "native-version": receipt["native-version"],
        "native-modules": [
            {
                "file": PureWindowsPath(m["path"]).name,
                "directory": PureWindowsPath(m["path"]).parent.name,
                **identity(m),
            }
            for m in receipt["native-modules-before"]
        ],
        "execution-identity": receipt["identity-before"],
        "queries": len(projected),
    }
    return source, projected, provenance


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    require(not args.output.exists(), "fresh output")
    cohorts = [project(args.input_root, version) for version in (1, 2)]
    require(sum(len(rows) for _, rows, _ in cohorts) == 9204, "total queries")
    args.output.mkdir()
    for source, rows, provenance in cohorts:
        shutil.copyfile(source, args.output / source.name)
        (args.output / (source.stem + "-observations.json")).write_text(
            json.dumps(rows, indent=2) + "\n"
        )
    (args.output / "provenance.json").write_text(
        json.dumps([p for _, _, p in cohorts], indent=2) + "\n"
    )
    print(
        json.dumps({"state": "finished", "queries": 9204, "output": str(args.output)})
    )


if __name__ == "__main__":
    main()
