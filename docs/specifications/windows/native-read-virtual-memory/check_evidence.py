"""Validate retained native observations and source-bound execution receipts."""

from pathlib import Path
import argparse
import hashlib
import json
import os
import re
import tarfile

BASE = "26ebabe4f8edefd87fc26630588cce0024b5bff8"
FRAME = "02b8f1cf7b693e6e379b2f8360e1dd9f646248f41dba8699d692209fc99f8303"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def identity(raw):
    return len(raw), hashlib.sha256(raw).hexdigest()


def pair(row):
    return row["bytes"], row["sha256"]


def load(path):
    return json.loads(path.read_bytes())


def check(directory, root):
    require(__debug__, "optimized Python is not an evidence checker")
    evidence = directory / "evidence"
    proof = load(directory / "proof.json")
    require(proof["base"] == BASE and proof["schema"] == 1, "parent/schema")
    paths = set()
    for row in proof["files"]:
        path = Path(row["path"])
        require(not path.is_absolute() and ".." not in path.parts, "evidence path")
        require(row["path"] not in paths, "duplicate evidence path")
        paths.add(row["path"])
        require(identity((directory / path).read_bytes()) == pair(row), str(path))
    actual = {
        str(path.relative_to(directory)).replace("\\", "/")
        for folder in ["evidence", "native", "producers"]
        for path in (directory / folder).rglob("*")
        if path.is_file()
    } | {"sources.json"}
    require(paths == actual, "complete evidence inventory")

    def receipt(name):
        return load(evidence / name)

    def log(row):
        name = row.get("log", row.get("file"))
        path = evidence / name
        if not path.exists():
            path = directory / "native" / name
        raw = path.read_bytes()
        require(identity(raw) == pair(row), "receipt log " + name)
        return raw.decode(errors="strict")

    frame_path = evidence / "read-memory-candidate5-source.json"
    require(identity(frame_path.read_bytes())[1] == FRAME, "compiler frame digest")
    frame = load(frame_path)
    require(frame["base"] == BASE, "source parent")
    require(
        len(frame["files"]) == 29173 and len(frame["overrides"]) == 40, "frame extent"
    )
    input_paths = set()
    for row in frame["files"]:
        require(row["path"] not in input_paths, "duplicate compiler input")
        input_paths.add(row["path"])
        path = root / row["path"]
        raw = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
        expected = pair(row)
        if os.name == "nt" and row["path"] == ".cargo/config.toml":
            require(
                expected
                == (
                    552,
                    "a198c84574684e95de13dbb1bdb7b6c918f90fdb490c649937f697fc9621f3cd",
                ),
                "host config",
            )
            expected = (
                374,
                "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
            )
        require(identity(raw) == expected, "compiler input " + row["path"])
    compiler_rows = {row["path"]: row for row in frame["files"]}
    for version in range(1, 8):
        for view in ["arm64", "x64", "x86", "x86-laa"]:
            name = f"native-read-virtual-memory-v{version}-four-views-{view}.log"
            path = (
                "docs/specifications/windows/native-read-virtual-memory/native/" + name
            )
            require(
                identity((directory / "native" / name).read_bytes())
                == pair(compiler_rows[path]),
                "native fixture is exact compiled input: " + name,
            )
    require(
        identity((evidence / frame["archive"]).read_bytes()) == pair(frame),
        "source archive",
    )
    with tarfile.open(evidence / frame["archive"], "r:gz") as bundle:
        expected = {row["path"]: row for row in frame["overrides"]}
        members = bundle.getmembers()
        require(
            len(members) == len(expected)
            and {m.name for m in members} == set(expected),
            "overlay paths",
        )
        for member in members:
            require(member.isfile(), "overlay kind")
            require(
                identity(bundle.extractfile(member).read())
                == pair(expected[member.name]),
                "overlay bytes",
            )

    source = load(directory / "sources.json")["signature"]
    require(
        identity(
            (
                root
                / "docs/specifications/windows/native-virtual-memory/phnt-ntmmapi.h"
            ).read_bytes()
        )
        == pair(source),
        "signature provenance",
    )
    native_rows = {}
    calls = 0
    for version, count in [(1, 1528), (2, 520), (3, 99), (4, 609), (6, 609), (7, 1920)]:
        launch = receipt(
            f"native-read-virtual-memory-v{version}-four-views-launch.json"
        )
        require(launch["state"] == "finished", "native launch terminal")
        require(
            launch["source-before"] == launch["source-after"], "native source stable"
        )
        require(
            identity(
                (
                    directory
                    / "producers"
                    / f"native-read-virtual-memory-v{version}.cpp"
                ).read_bytes()
            )
            == pair(launch["source-before"]),
            "producer identity",
        )
        require(
            launch["runtime-modules-before"] == launch["runtime-modules-after"]
            and len(launch["runtime-modules-before"]) == 8,
            "native DLL stability",
        )
        require(
            len(launch["steps"]) == 16 and len(launch["outcomes"]) == 4,
            "native complete four views",
        )
        for step in launch["steps"]:
            require(step["exit-code"] == 0, "native step exit")
            raw = log(step)
            if step["name"].endswith(("-before", "-after")):
                pe = json.loads(raw)
                arch = pe["profile"]
                require(
                    pe["machine"]
                    == {"arm64": 0xAA64, "x64": 0x8664, "x86": 0x14C, "x86-laa": 0x14C}[
                        arch
                    ],
                    "PE machine",
                )
                require(pe["large-address-aware"] == (arch != "x86"), "PE LAA")
        for outcome in launch["outcomes"]:
            require(
                outcome["exit-code"] == 0
                and outcome["executable-before"] == outcome["executable-after"],
                "native executable stable",
            )
            profile = outcome["profile"]
            step = next(row for row in launch["steps"] if row["name"] == profile)
            text = log(step)
            rows = [
                json.loads(line) for line in text.splitlines() if line.startswith("{")
            ]
            require(
                len(rows) == count and f"END calls={count} all-returned=1" in text,
                "native call selection",
            )
            for index, row in enumerate(rows):
                require(
                    row["id"] == index
                    and row["exception"] == 0
                    and row["last-error"] == 0x12345678,
                    "native row envelope",
                )
                extent = row.get("extent", 3 * 4096)
                for change in row["changes"]:
                    data = bytes.fromhex(change["data"])
                    require(
                        0 <= change["role"] < 3
                        and change["offset"] + len(data) * change.get("repeat", 1)
                        <= extent,
                        "lossless change bounds",
                    )
            native_rows[(version, profile)] = rows
            calls += count
        require(
            native_rows[(version, "arm64")] == native_rows[(version, "x64")],
            "64-bit view equality",
        )
        require(
            native_rows[(version, "x86")] == native_rows[(version, "x86-laa")],
            "32-bit view equality",
        )
    require(calls == 21140, "native total")
    repeat = receipt("native-read-repeat-v1.json")
    require(
        repeat["state"] == "finished"
        and repeat["exit-code"] == 0
        and len(repeat["steps"]) == 16,
        "native repeat complete",
    )
    require(
        repeat["runtime-modules-before"] == repeat["runtime-modules-after"],
        "repeat DLL identity",
    )
    for step in repeat["steps"]:
        require(
            step["exit-code"] == 0 and step["complete-nonresidency-row-match"],
            "repeat status",
        )
        require(
            step["executable-before"] == step["executable-after"], "repeat executable"
        )
        rows = [
            json.loads(line) for line in log(step).splitlines() if line.startswith("{")
        ]
        original = native_rows[(step["version"], step["profile"])]
        projection = lambda rs: [
            {
                k: v
                for k, v in r.items()
                if k not in ["resident-before", "resident-after"]
            }
            for r in rs
        ]
        require(
            projection(rows) == projection(original), "native repeated complete rows"
        )

    alignment_repeat = receipt("native-read-repeat-v3.json")
    require(
        alignment_repeat["state"] == "finished"
        and alignment_repeat["exit-code"] == 0
        and len(alignment_repeat["steps"]) == 8,
        "alignment repeat complete",
    )
    require(
        alignment_repeat["runtime-modules-before"]
        == alignment_repeat["runtime-modules-after"],
        "alignment repeat module stability",
    )
    for step in alignment_repeat["steps"]:
        require(
            step["exit-code"] == 0
            and step["complete-nonresidency-row-match"]
            and step["executable-before"] == step["executable-after"],
            "alignment repeat execution",
        )
        rows = [
            json.loads(line) for line in log(step).splitlines() if line.startswith("{")
        ]
        original = native_rows[(step["version"], step["profile"])]
        projection = lambda rs: [
            {
                k: v
                for k, v in r.items()
                if k not in ["resident-before", "resident-after"]
            }
            for r in rs
        ]
        require(
            projection(rows) == projection(original), "alignment repeated complete rows"
        )
    alignment_cleanup = receipt("native-read-probe-cleanup-v3.json")
    require(
        alignment_cleanup["state"] == "finished"
        and alignment_cleanup["active-probe-processes"] == []
        and len(alignment_cleanup["directories"]) == 2
        and all(row["removed"] for row in alignment_cleanup["directories"]),
        "alignment probe generated cleanup",
    )
    alignment_regression = receipt("native-read-alignment-before-v1.json")
    require(
        identity((evidence / alignment_regression["source"]["file"]).read_bytes())
        == pair(alignment_regression["source"]),
        "alignment regression source identity",
    )
    require(
        alignment_regression["core-before"] == alignment_regression["core-after"],
        "alignment before core stable",
    )
    require(
        alignment_regression["steps"][0]["exit-code"] == 0
        and alignment_regression["steps"][1]["exit-code"] == 101,
        "actual handler alignment regression",
    )
    for step in alignment_regression["steps"]:
        log(step)
    corrected_alignment = receipt("native-read-alignment-after-v1.json")
    require(
        corrected_alignment["source-frame-sha256"] == FRAME
        and corrected_alignment["core-before"] == corrected_alignment["core-after"],
        "corrected alignment core binding",
    )
    for step in corrected_alignment["steps"]:
        require(step["exit-code"] == 0, "corrected alignment regression")
        log(step)
    rtl = receipt("native-read-virtual-memory-v5-four-views-launch.json")
    require(
        rtl["state"] == "finished" and rtl["source-before"] == rtl["source-after"],
        "native RTL source",
    )
    require(
        rtl["runtime-modules-before"] == rtl["runtime-modules-after"],
        "native RTL modules",
    )
    require(
        identity(
            (directory / "producers/native-read-virtual-memory-v5.cpp").read_bytes()
        )
        == pair(rtl["source-before"]),
        "RTL producer identity",
    )
    require(len(rtl["steps"]) == 16 and len(rtl["outcomes"]) == 4, "four RTL views")
    rtl_rows = []
    for step in rtl["steps"]:
        require(step["exit-code"] == 0, "RTL step exit")
        text = log(step)
        if step["name"] in ["arm64", "x64", "x86", "x86-laa"]:
            rows = [
                json.loads(line) for line in text.splitlines() if line.startswith("{")
            ]
            require(
                len(rows) == 7 and [row["id"] for row in rows] == list(range(7)),
                "RTL selection",
            )
            require(
                all(row["last-error"] == 0x12345678 for row in rows), "RTL last error"
            )
            require(
                next(row["dos-error"] for row in rows if row["status"] == 0x8000000D)
                == 299,
                "independent partial-copy translation",
            )
            rtl_rows.append(rows)
    require(all(rows == rtl_rows[0] for rows in rtl_rows), "RTL four-view equality")
    mapping_regression = receipt("native-read-status-map-before-v2.json")
    require(
        mapping_regression["core-before"] == mapping_regression["core-after"],
        "mapping regression core stability",
    )
    require(
        mapping_regression["steps"][0]["exit-code"] == 0
        and mapping_regression["steps"][1]["exit-code"] == 101,
        "mapping regression selection",
    )
    require(
        "observed-dos-error=317 expected-native=299"
        in log(mapping_regression["steps"][1]),
        "mapping old failure observed",
    )
    rtl_repeat = receipt("native-read-repeat-v2.json")
    require(
        rtl_repeat["state"] == "finished"
        and rtl_repeat["exit-code"] == 0
        and len(rtl_repeat["steps"]) == 4,
        "RTL repeat complete",
    )
    require(
        rtl_repeat["runtime-modules-before"]
        == rtl_repeat["runtime-modules-after"]
        == rtl["runtime-modules-before"],
        "RTL repeat modules",
    )
    for step in rtl_repeat["steps"]:
        require(
            step["exit-code"] == 0
            and step["calls"] == 7
            and step["complete-nonresidency-row-match"],
            "RTL repeat selection",
        )
        require(
            step["executable-before"] == step["executable-after"],
            "RTL repeat executable stable",
        )
        rows = [
            json.loads(line) for line in log(step).splitlines() if line.startswith("{")
        ]
        require(rows == rtl_rows[0], "RTL repeated complete rows")
    rtl_cleanup = receipt("native-read-probe-cleanup-v2.json")
    require(
        rtl_cleanup["state"] == "finished"
        and rtl_cleanup["active-probe-processes"] == []
        and len(rtl_cleanup["directories"]) == 1
        and rtl_cleanup["directories"][0]["removed"],
        "RTL generated directory cleanup",
    )
    corrected_mapping = receipt("native-read-status-map-after-v2.json")
    require(
        corrected_mapping["source-frame-sha256"] == FRAME
        and corrected_mapping["core-before"] == corrected_mapping["core-after"],
        "corrected owning-core mapping identity",
    )
    for step in corrected_mapping["steps"]:
        require(step["exit-code"] == 0, "corrected mapping exit")
        text = log(step)
        if step["name"] == "execute":
            require(
                "observed-dos-error=299 expected-native=299" in text,
                "corrected native mapping",
            )
    owning_manifest = receipt("read-memory-owning-source-hashes-candidate5.json")
    require(
        owning_manifest["source-frame"]["sha256"] == FRAME
        and len(owning_manifest["sources"]) == 3978,
        "owning compiler frame",
    )
    source_rows = {row["path"]: row for row in frame["files"]}
    for row in owning_manifest["sources"]:
        require(
            row["path"] in source_rows and pair(row) == pair(source_rows[row["path"]]),
            "owning source is final compiler input: " + row["path"],
        )
    regression = receipt("native-read-parent-regression-v1.json")
    require(
        regression["parent"] == BASE and regression["exit-code"] == 101,
        "parent regression",
    )
    require(
        "NtReadVirtualMemory (0x23)" in log(regression["log"])
        and "0 passed; 1 failed;" in log(regression["log"]),
        "old path observed",
    )
    restoration = receipt("read-memory-worktree-restoration-v1.json")
    require(
        restoration["state"] == "finished"
        and restoration["baseline"] == BASE
        and restoration["source-frame-sha256"] == FRAME
        and restoration["verified-compiler-inputs"] == 29173
        and restoration["overlay-files"] == 40
        and restoration["cause-of-disappearance"] == "unknown",
        "exact worktree restoration",
    )
    interrupted = receipt("read-memory-linux-candidate5-gates.json")
    require(
        interrupted["state"] == "failed"
        and interrupted["exit-code"] == 1
        and interrupted["manifest-sha256"] == FRAME
        and interrupted["step"] == "full"
        and "FileNotFoundError" in interrupted["error"],
        "retain interrupted Linux integrity check",
    )
    require(
        [step["name"] for step in interrupted["steps"]]
        == ["ordinals", "focus", "full"],
        "interrupted Linux selection",
    )
    for step in interrupted["steps"]:
        require(step["exit-code"] == 0, "interrupted run test command exit")
        log(step)
    summaries = {}
    for host, full in [("macos", 8196), ("linux", 8191)]:
        name = (
            "read-memory-linux-candidate5-recovery-v2-gates.json"
            if host == "linux"
            else "read-memory-macos-candidate5-gates.json"
        )
        gates = receipt(name)
        if host == "linux":
            require(
                gates["restoration"] == "read-memory-worktree-restoration-v1.json"
                and gates["supersedes"] == "read-memory-linux-candidate5-gates.json",
                "Linux complete rerun restoration chain",
            )
        require(
            gates["state"] == "finished"
            and gates["exit-code"] == 0
            and gates["manifest-sha256"] == FRAME,
            "Unix gates source/terminal",
        )
        require(
            [s["name"] for s in gates["steps"]]
            == ["ordinals", "focus", "full", "capi", "integration", "all-targets"],
            "Unix gates selection",
        )
        for step, count in zip(gates["steps"], [4, 12, full, 183, 544, None]):
            text = log(step)
            require(step["exit-code"] == 0, "Unix exit")
            if count is not None:
                require(f"{count} passed; 0 failed;" in text, "Unix pass count")
            if step["name"] == "full":
                require(
                    "2 ignored; 0 measured; 0 filtered out;" in text, "Unix full filter"
                )
        summaries[host] = {
            "focus": 12,
            "full-passed": full,
            "full-failed": 0,
            "full-ignored": 2,
        }
    windows = receipt("read-memory-candidate5-windows.json")
    require(
        windows["state"] == "finished" and windows["exit-code"] == 0,
        "Windows gates complete",
    )
    require(
        windows["manifest"]["sha256"] == FRAME and windows["source-inputs"] == 29173,
        "Windows source",
    )
    require(
        windows["test-image-before"] == windows["test-image-after"],
        "Windows exact test image",
    )
    failures = set(receipt("mutant-native-baseline-failures.json")["failures"])
    require(
        len(failures) == 5 and set(windows["exact-full-failures"]) == failures,
        "Windows exact remaining failures",
    )
    by_name = {row["name"]: row for row in windows["steps"]}
    for name, count in [
        ("ordinals", 4),
        ("focus", 13),
        ("capi", 183),
        ("integration", 4),
        ("all-targets", None),
    ]:
        text = log(by_name[name])
        require(by_name[name]["exit-code"] == 0, "Windows gate exit")
        if count is not None:
            require(f"{count} passed; 0 failed;" in text, "Windows gate count")
    full = log(by_name["full"])
    require(
        by_name["full"]["exit-code"] == 101
        and "7668 passed; 5 failed; 2 ignored; 0 measured; 0 filtered out;" in full,
        "Windows full truthful result",
    )
    observed = set(
        re.findall(r"^    (\S+)\r?$", full[full.rfind("\nfailures:\n") :], re.M)
    )
    require(observed == failures, "Windows raw failure selection")
    summaries["windows"] = {
        "focus": 13,
        "full-passed": 7668,
        "full-failed": 5,
        "full-ignored": 2,
    }

    for host in ["macos", "linux", "windows"]:
        name = (
            f"{host}-read-memory-candidate5-owning-gates-v2.json"
            if host != "windows"
            else "read-memory-native-owning-candidate5.json"
        )
        gates = receipt(name)
        require(
            gates["state"] == "finished"
            and gates["exit-code"] == 0
            and gates["archive-identities-stable"],
            "owning consumers complete",
        )
        for step in gates["steps"]:
            require(step["exit-code"] == 0, "owning step exit")
            log(step)
        prefix = (
            f"{host}-read-memory-candidate5"
            if host != "windows"
            else "native-read-memory-candidate5"
        )
        before = receipt(
            prefix
            + (
                "-archives-before.json"
                if host != "windows"
                else "-pre-cpp-archives.json"
            )
        )
        after = receipt(
            prefix
            + (
                "-archives-after.json"
                if host != "windows"
                else "-final-owning-archive-identities.json"
            )
        )
        require(
            before["archives"] == after["archives"] and len(before["archives"]) == 2,
            "actual owning archives stable",
        )
        for archive in before["archives"]:
            require(
                archive["structural-member-walk"] == "pass"
                and archive["members"] > 0
                and archive["bytes"] > 0,
                "archive structural walk",
            )
    quality = receipt("read-memory-quality-v4.json")
    require(
        quality["state"] == "finished" and quality["source-frame-sha256"] == FRAME,
        "formatter/Clippy complete",
    )
    for step in quality["steps"]:
        require(step["exit-code"] == 0, "formatter/Clippy exit")
        log(step)
    cleanup = receipt("native-read-probe-cleanup-v1.json")
    require(
        cleanup["state"] == "finished" and cleanup["active-probe-processes"] == [],
        "native probe cleanup",
    )
    require(
        len(cleanup["directories"]) == 4
        and all(row["removed"] for row in cleanup["directories"]),
        "native generated directory lifecycle",
    )

    # These observers use the actual retained owning artifacts. Their process
    # status is independently checked; an observer exit code of zero is not a
    # claim that the ordinary guest completed.
    archives = receipt(
        "native-read-memory-candidate5-final-owning-archive-identities.json"
    )["archives"]
    core = next(row for row in archives if row["path"].endswith(".rlib"))
    carrier = next(row for row in archives if row["path"].endswith(".lib"))
    ordinary = receipt("native-read-next-frontier-v3.json")
    require(
        ordinary["state"] == "finished" and ordinary["exit-code"] == 0,
        "owning Rust observer complete",
    )
    for field in ["core", "observer", "executable", "runtime"]:
        require(
            ordinary[field + "-before"] == ordinary[field + "-after"],
            "Rust observer stable " + field,
        )
    require(
        pair(ordinary["core-before"]) == pair(core),
        "actual owning core observer binding",
    )
    for step in ordinary["steps"]:
        require(step["exit-code"] == 0, "Rust observer exit")
        text = log(step)
        if step["name"] == "smoke":
            require("Complete(Exited(0))" in text, "Rust smoke exited")
        if step["name"] == "whoami":
            require(
                "NtOpenKey" in text
                and "registry key outside selected runtime snapshot" in text
                and "native NTDLL service NtReadVirtualMemory" not in text,
                "Rust next frontier",
            )
    for outcome in ordinary["outcomes"]:
        require(
            outcome["program-before"] == outcome["program-after"]
            and outcome["native-before"] == outcome["native-after"]
            and outcome["native-before"]["exit-code"] == 0,
            "ordinary native control stable",
        )
    api = receipt("read-memory-owning-capi-candidate5.json")
    require(
        api["state"] == "finished" and api["exit-code"] == 0,
        "C++/C API ordinary observer",
    )
    for field in [
        "native",
        "programs",
        "runtime",
        "source",
        "carrier",
        "adapter-inputs",
        "executable",
    ]:
        require(
            api[field + "-before"] == api[field + "-after"], "C API stable " + field
        )
    require(
        pair(api["carrier-before"]) == pair(carrier),
        "actual owning carrier observer binding",
    )
    for step in api["steps"]:
        require(step["exit-code"] == 0, "C API observer exit")
        log(step)
    smoke, whoami = api["outcomes"]
    require(
        smoke["reason"] == 4
        and smoke["exit-code"] == 0
        and smoke["stdout-hex"] == smoke["stderr-hex"] == "",
        "C API smoke exit/output",
    )
    require(
        whoami["reason"] == 5
        and "NtOpenKey" in whoami["diagnostic"]
        and "registry key outside selected runtime snapshot" in whoami["diagnostic"],
        "C API next frontier, not false guest exit",
    )
    require(
        all(row["exit-code"] == 0 for row in api["native-before"]),
        "native ordinary controls succeed",
    )
    return {
        "native-calls": calls,
        "native-repeat-calls": calls,
        "rtl-calls": 28,
        "rtl-repeat-calls": 28,
        "compiler-inputs": 29173,
        "gates": summaries,
        "owning-consumer-hosts": 3,
        "independent-x64-kernel": "unknown",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    here = Path(__file__).resolve().parent
    parser.add_argument("--evidence-dir", type=Path, default=here)
    parser.add_argument("--source-root", type=Path, default=here.parents[3])
    args = parser.parse_args()
    print(json.dumps(check(args.evidence_dir, args.source_root), indent=2))


if __name__ == "__main__":
    main()
