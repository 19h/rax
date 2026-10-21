"""Audit fixed NLS sorting controls without requiring a Windows host."""

from pathlib import Path
import argparse
import hashlib
import json


def need(condition, reason):
    if not condition:
        raise ValueError(reason)


def identity(path):
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
            size += len(block)
    return {"bytes": size, "sha256": digest.hexdigest()}


def pair(row):
    return {key: row[key] for key in ("bytes", "sha256")}


def fields(line):
    return dict(part.split("=", 1) for part in line.split()[1:])


def check(directory, source=None, live=False):
    producer = json.loads((directory / "native-producer.json").read_text())
    need(
        identity(directory / producer["launch"])
        == {"bytes": producer["launch-bytes"], "sha256": producer["launch-sha256"]},
        "launch identity",
    )
    launch = json.loads((directory / producer["launch"]).read_text())
    need(
        launch["state"] == "finished" and launch["exit-code"] == 0, "native completion"
    )
    need(launch["source-before"] == launch["source-after"], "native source stability")
    need(
        identity(directory / producer["source"]) == pair(launch["source-before"])
        and launch["source-before"]["sha256"] == producer["source-sha256"],
        "producer source identity",
    )
    modules = launch["runtime-modules-after"]
    need(
        launch["runtime-modules-before"] == modules and len(modules) == 8,
        "runtime DLL stability",
    )
    need(
        launch["compiler-environment"]["sha256"]
        == "e6ca43bcdcbd3067b5de05d051259d4d3abf0bfa5283a98daac14fd508a05fad",
        "selected native compiler environment",
    )
    for step in launch["steps"]:
        need(step["exit-code"] == 0, "native stage failure")
        need(
            identity(directory / "native" / step["log"]) == pair(step), "stage identity"
        )
    profiles = ["arm64", "x64", "x86", "x86-laa"]
    need([row["profile"] for row in launch["outcomes"]] == profiles, "execution views")
    live_files = {row["path"]: row for row in [launch["source-after"], *modules]}
    expected_values = None
    opens = 0
    value_observations = 0
    for outcome in launch["outcomes"]:
        profile = outcome["profile"]
        need(
            outcome["exit-code"] == 0
            and outcome["executable-before"] == outcome["executable-after"],
            "native image stability",
        )
        exe = outcome["executable-after"]
        live_files[exe["path"]] = exe
        prefix = "native-nls-sorting-v1-four-views-" + profile
        before = json.loads(
            (directory / "native" / (prefix + "-before.log")).read_text()
        )
        after = json.loads((directory / "native" / (prefix + "-after.log")).read_text())
        need(before == after and pair(before) == pair(exe), "PE identity")
        need(
            before["machine"]
            == {"arm64": 0xAA64, "x64": 0x8664, "x86": 0x14C, "x86-laa": 0x14C}[
                profile
            ],
            "PE machine",
        )
        need(profile != "x86-laa" or before["large-address-aware"], "x86 LAA control")
        lines = (directory / "native" / outcome["log"]).read_text().splitlines()
        for view in ("00000000", "00000100", "00000200"):
            selected = [
                fields(line)
                for line in lines
                if line.startswith("SNAPSHOT ") and fields(line)["view"] == view
            ]
            need(
                len(selected) == 1 and selected[0]["open"] == "0",
                "fixed native key present",
            )
            metadata = [
                fields(line)
                for line in lines
                if line.startswith("METADATA ") and fields(line)["view"] == view
            ]
            need(
                len(metadata) == 1
                and metadata[0]["children"] == "0"
                and metadata[0]["values"] == "10",
                "native key metadata",
            )
            values = [
                fields(line)
                for line in lines
                if line.startswith("VALUE ") and fields(line)["view"] == view
            ]
            values = [
                {key: value[key] for key in ("name", "kind", "data")}
                for value in values
            ]
            need(
                len(values) == 10 and len({value["name"] for value in values}) == 10,
                "raw value coverage",
            )
            need(
                all(
                    value["kind"] == "1"
                    and len(value["name"]) % 4 == 0
                    and len(value["data"]) % 4 == 0
                    and bytes.fromhex(value["data"]).endswith(b"\0\0")
                    for value in values
                ),
                "raw UTF-16 registry values",
            )
            if expected_values is None:
                expected_values = values
            need(values == expected_values, "raw values differ between native views")
            value_observations += len(values)
        rows = [fields(line) for line in lines if line.startswith("OPEN ")]
        need(len(rows) == 80, "native open matrix size")
        keys = set()
        for row in rows:
            sample, path, access = (
                int(row["sample"]),
                int(row["path"]),
                int(row["desired"], 16),
            )
            need(
                sample in (0, 1)
                and path in range(8)
                and access in (1, 3, 0x20019, 0x20119, 0x20219),
                "native input domain",
            )
            keys.add((sample, path, access))
            need(
                int(row["status"], 16) == (0 if path in (0, 1, 5) else 0xC0000034),
                "native path status",
            )
        need(len(keys) == 80, "duplicate or missing native input")
        summaries = [fields(line) for line in lines if line.startswith("SUMMARY ")]
        need(
            len(summaries) == 1
            and summaries[0]["cases"] == "80"
            and summaries[0]["handles-before"] == summaries[0]["handles-after"],
            "native handle balance",
        )
        opens += len(rows)
    need(opens == producer["native-open-calls"] == 320, "total native opens")
    need(
        value_observations == producer["raw-value-observations"] == 120,
        "total raw value observations",
    )
    if live:
        for path, row in live_files.items():
            need(identity(Path(path)) == pair(row), "live native file changed: " + path)
    if source is not None:
        ledger = json.loads(
            (directory / "validation/nls-sorting-candidate1-source.json").read_text()
        )
        need(
            len(ledger["files"]) == 26902 and len(ledger["overrides"]) == 11,
            "source graph coverage",
        )
        for row in ledger["overrides"]:
            need(
                identity(source / row["path"]) == pair(row),
                "current source overlay: " + row["path"],
            )
        prefix = json.loads(
            (directory / "validation/nls-sorting-red-prefix-binding.json").read_text()
        )
        original = (directory / "validation/nls-sorting-red-tests.rs").read_bytes()
        final = (
            source / "src/user/windows/registry/installed/nls_sorting_tests.rs"
        ).read_bytes()
        need(
            final.startswith(original)
            and hashlib.sha256(original).hexdigest() == prefix["red-test-sha256"],
            "red native test prefix",
        )
    return {
        "state": "verified",
        "native-open-calls": opens,
        "raw-value-observations": value_observations,
        "execution-views": 4,
        "registry-views-per-execution": 3,
        "live-native-files": len(live_files) if live else 0,
        "scope": producer["scope"],
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--directory", type=Path, default=Path(__file__).parent)
    parser.add_argument("--source", type=Path)
    parser.add_argument("--live-native", action="store_true")
    args = parser.parse_args()
    print(json.dumps(check(args.directory, args.source, args.live_native), indent=2))
