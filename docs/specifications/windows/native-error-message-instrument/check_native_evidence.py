"""Verify the fixed-root native observation and its source/byte provenance."""

from pathlib import Path
import argparse
import hashlib
import json
import os

PREFIX = "native-error-message-instrument-20261021-v1"
SOURCE_SHA = "3983e87dfefee7df62551f4269a04b5c2d1f2df4682749dac8d0af562b4896e8"
LEDGER_SHA = "c487efef544b8a75cc678d5781e460583c3a34870fb728bb937dc1ea35ba72b1"
PROFILES = ["arm64", "x64", "x86", "x86-laa"]


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def identity(path):
    h = hashlib.sha256()
    size = 0
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            size += len(block)
            h.update(block)
    return {"bytes": size, "sha256": h.hexdigest()}


def pair(row):
    return {key: row[key] for key in ["bytes", "sha256"]}


def fields(line):
    return dict(word.split("=", 1) for word in line.split()[1:] if "=" in word)


def _producer(directory, prefix, source_sha, spellings):
    per_view = spellings * 18
    native = directory / "native"
    receipt = json.loads((native / (prefix + "-four-views-launch.json")).read_text())
    require(
        receipt["state"] == "finished" and receipt["exit-code"] == 0,
        "native completion",
    )
    require(
        receipt["source-before"] == receipt["source-after"], "native source stability"
    )
    require(
        identity(native / (prefix + ".cpp")) == pair(receipt["source-after"]),
        "producer source bytes",
    )
    require(receipt["source-after"]["sha256"] == source_sha, "fixed native producer")
    require(
        receipt["runtime-modules-before"] == receipt["runtime-modules-after"],
        "runtime input stability",
    )
    require(
        receipt["compiler-environment"]["sha256"]
        == "e6ca43bcdcbd3067b5de05d051259d4d3abf0bfa5283a98daac14fd508a05fad",
        "compiler environment",
    )
    require(
        [row["profile"] for row in receipt["outcomes"]] == PROFILES,
        "four execution views",
    )
    for step in receipt["steps"]:
        require(
            step["exit-code"] == 0 and identity(native / step["log"]) == pair(step),
            "native stage identity",
        )
    files = {
        row["path"]: row
        for row in [receipt["source-after"], *receipt["runtime-modules-after"]]
    }
    calls = 0
    grants = {0, 1, 2, 0x20000, 0x20019, 0xF003F, 0x2000000, 0x20119, 0x20219}
    expected = {
        (spelling, flags, grant)
        for spelling in range(spellings)
        for flags in [0, 0x40]
        for grant in grants
    }
    for view in receipt["outcomes"]:
        profile = view["profile"]
        require(
            view["exit-code"] == 0
            and view["executable-before"] == view["executable-after"],
            "native executed image stability",
        )
        executable = view["executable-after"]
        files[executable["path"]] = executable
        before = json.loads(
            (native / (prefix + "-four-views-" + profile + "-before.log")).read_text()
        )
        after = json.loads(
            (native / (prefix + "-four-views-" + profile + "-after.log")).read_text()
        )
        require(before == after and pair(before) == pair(executable), "PE identity")
        require(
            before["machine"]
            == {"arm64": 0xAA64, "x64": 0x8664, "x86": 0x14C, "x86-laa": 0x14C}[
                profile
            ],
            "PE architecture",
        )
        require(profile != "x86-laa" or before["large-address-aware"], "LAA execution")
        lines = (native / view["log"]).read_text().splitlines()
        rows = [fields(line) for line in lines if line.startswith("OPEN ")]
        actual = {
            (int(row["spelling"]), int(row["flags"], 16), int(row["grant"], 16))
            for row in rows
        }
        require(len(rows) == per_view and actual == expected, "native matrix extent")
        for row in rows:
            require(row["status"] == "C0000034", "native fixed-root absence")
            require(
                row["exception"] == "00000000" and int(row["handle"], 16) == 0,
                "failure output handle",
            )
            require(row["last-error"] == "12345678", "LastError preservation")
        summary = [fields(line) for line in lines if line.startswith("SUMMARY ")]
        require(len(summary) == 1, "owned cleanup summary")
        summary = summary[0]
        require(
            summary["calls"] == str(per_view)
            and summary["opened"] == summary["closed"] == "0"
            and summary["handles-before"] == summary["handles-after"],
            "owned native handle accounting",
        )
        calls += len(rows)
    require(calls == per_view * 4, "producer call extent")
    return files, calls


def check(directory, source=None, live=False):
    files, roots = _producer(directory, PREFIX, SOURCE_SHA, 4)
    extra, descendants = _producer(
        directory,
        "native-error-message-instrument-20261022-v2",
        "2fa21a0158480836f10ecb3764a424e627f954ea3305dafafd2915bdb12770ed",
        8,
    )
    for path, row in extra.items():
        if path in files:
            require(pair(files[path]) == pair(row), "cross-producer input identity")
        files[path] = row
    calls = roots + descendants
    if live:
        for row in files.values():
            require(
                identity(Path(row["path"])) == pair(row), "live input " + row["path"]
            )
    count = 0
    if source:
        ledger = directory / "validation/eri-candidate1-source.json"
        require(identity(ledger)["sha256"] == LEDGER_SHA, "compiled source ledger")
        manifest = json.loads(ledger.read_text())
        for row in manifest["files"]:
            path = source / row["path"]
            expected_identity = pair(row)
            if os.name == "nt" and row["path"] == ".cargo/config.toml":
                expected_identity = {
                    "bytes": 374,
                    "sha256": "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
                }
            if path.is_symlink():
                raw = os.readlink(path).encode()
                actual_identity = {
                    "bytes": len(raw),
                    "sha256": hashlib.sha256(raw).hexdigest(),
                }
            else:
                actual_identity = identity(path)
            require(
                actual_identity == expected_identity, "compiled source " + row["path"]
            )
            count += 1
        require(count == 28816, "frozen source extent")
    return {
        "state": "verified",
        "native-open-observations": calls,
        "native-executables": 8,
        "unique-live-inputs": len(files),
        "source-inputs": count,
        "scope": "Four execution views on one ARM64 kernel; source-specific observed absence, not universal absence or independent physical x64 validation.",
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path)
    parser.add_argument("--live-native", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = (
        json.dumps(
            check(Path(__file__).resolve().parent, args.source, args.live_native),
            indent=2,
        )
        + "\n"
    )
    print(result, end="")
    if args.output:
        args.output.write_text(result)


if __name__ == "__main__":
    main()
