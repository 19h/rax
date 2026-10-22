"""Verify the captured Display-policy oracle and frozen compiler envelope."""

import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import tarfile


HERE = Path(__file__).resolve().parent
PROFILES = {"arm64": 8, "x64": 8, "x86": 4, "x86-laa": 4}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def identity(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
    return {"bytes": path.stat().st_size, "sha256": digest.hexdigest()}


def matches(path, expected):
    actual = identity(path)
    require(all(actual[key] == expected[key] for key in actual), str(path))


def load(path):
    return json.loads(path.read_text())


def fields(line):
    return dict(word.split("=", 1) for word in line.split()[1:])


def native_rows(text, profile):
    lines = text.splitlines()
    require(
        lines[0] == f"PROFILE {profile} POINTER_BYTES {PROFILES[profile]}", "profile"
    )
    rows = [fields(line) for line in lines if line.startswith("OPEN ")]
    require(len(rows) == 144, "native open count")
    seen = set()
    grants = set()
    for row in rows:
        key = (int(row["spelling"]), int(row["flags"], 16), int(row["grant"], 16))
        require(key not in seen, "duplicate native case")
        seen.add(key)
        grants.add(key[2])
        require(int(row["status"], 16) == 0xC0000034, "native absence status")
        require(int(row["exception"], 16) == 0, "native exception")
        require(int(row["handle"], 16) == 0, "failed native handle")
        require(int(row["last-error"], 16) == 0x12345678, "native LastError")
    require(len(grants) == 9 and 0x20019 in grants, "native grants")
    require(
        seen == set(itertools.product(range(4), [0, 0x40, 0x200, 0x240], grants)),
        "native matrix",
    )
    summaries = [fields(line) for line in lines if line.startswith("SUMMARY ")]
    require(len(summaries) == 1, "native summary")
    summary = summaries[0]
    require(
        [int(summary[key]) for key in ["calls", "opened", "closed"]] == [144, 0, 0],
        "native summary counts",
    )
    require(summary["handles-before"] == summary["handles-after"], "native handle leak")
    return grants


def check_native(base):
    native = base / "native"
    launch = load(native / "native-display-policy-20261022-v1-four-views-launch.json")
    require(launch["state"] == "finished" and launch["exit-code"] == 0, "native launch")
    require(launch["source-before"] == launch["source-after"], "producer changed")
    require(
        launch["runtime-modules-before"] == launch["runtime-modules-after"],
        "native runtime changed",
    )
    matches(native / "native-display-policy-20261022-v1.cpp", launch["source-before"])
    for step in launch["steps"]:
        require(step["exit-code"] == 0, "native step")
        matches(native / step["log"], step)
    require(
        {row["profile"] for row in launch["outcomes"]} == set(PROFILES), "native views"
    )
    observed = []
    for row in launch["outcomes"]:
        require(
            row["exit-code"] == 0
            and row["executable-before"] == row["executable-after"],
            "native executable changed",
        )
        observed.append(native_rows((native / row["log"]).read_text(), row["profile"]))
    require(all(item == observed[0] for item in observed), "native view grants differ")
    audit = load(base / "validation/display-final-native-audit-v1.json")
    require(
        audit["state"] == "finished" and audit["verified"] and audit["exit_code"] == 0,
        "final native audit",
    )
    require(audit["source-inputs"] == 29015, "native source envelope")
    require(
        {row["profile"] for row in audit["native-after"]} == set(PROFILES),
        "final native views",
    )
    for row in audit["native-after"]:
        matches(base / "validation" / row["log"], row["log-identity"])
        native_rows((base / "validation" / row["log"]).read_text(), row["profile"])
    return audit


def check_source(base, source):
    validation = base / "validation"
    frame = load(validation / "display-final-compiled-overrides-v1.json")
    require(
        frame["base-git"] == "39c678a46b069dba9a036524f2e38b096f043dbd",
        "compiler baseline",
    )
    ledger = validation / frame["source-ledger"]
    require(
        identity(ledger)["sha256"] == frame["source-ledger-sha256"], "compiler ledger"
    )
    inputs = load(ledger)["files"]
    require(
        len(inputs) == 29015 and len({row["path"] for row in inputs}) == 29015,
        "compiler input count",
    )
    package = validation / frame["archive"]
    matches(package, frame)
    expected = {row["path"]: row for row in frame["overrides"]}
    require(len(expected) == 25, "compiler override count")
    with tarfile.open(package) as archive:
        members = archive.getmembers()
        require(
            len(members) == len(expected)
            and {item.name for item in members} == set(expected),
            "compiler archive membership",
        )
        for member in members:
            require(
                member.isfile()
                and not Path(member.name).is_absolute()
                and ".." not in Path(member.name).parts,
                "compiler member path",
            )
            raw = archive.extractfile(member).read()
            row = expected[member.name]
            require(
                (len(raw), hashlib.sha256(raw).hexdigest())
                == (row["bytes"], row["sha256"]),
                "compiler member bytes",
            )
    if source is not None:
        for row in inputs:
            path = source / row["path"]
            raw = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
            expected_pair = (row["bytes"], row["sha256"])
            if os.name == "nt" and row["path"] == ".cargo/config.toml":
                expected_pair = (
                    374,
                    "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
                )
            require(
                (len(raw), hashlib.sha256(raw).hexdigest()) == expected_pair,
                "compiler source " + row["path"],
            )


def check_catalog(base):
    proof = load(base / "proof.json")
    for row in proof["files"]:
        path = base / row["path"]
        require(path.resolve().is_relative_to(base.resolve()), "evidence path")
        matches(path, row)
    return proof


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path)
    parser.add_argument("--live-native", action="store_true")
    args = parser.parse_args()
    proof = check_catalog(HERE)
    check_source(HERE, args.source)
    audit = check_native(HERE)
    if args.live_native:
        require(os.name == "nt", "live native audit requires Windows")
        relocation = audit["relocation"]
        for row in audit["checked"]:
            path = Path(row["path"])
            if path.is_relative_to(Path(relocation["old"])):
                path = Path(relocation["new"]) / path.relative_to(
                    Path(relocation["old"])
                )
            matches(path, row)
    print(
        f"verified {len(proof['files'])} evidence files; 29015 compiler inputs; 576 native opens before and after"
    )


if __name__ == "__main__":
    main()
