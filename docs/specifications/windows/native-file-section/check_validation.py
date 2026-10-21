"""Audit the file-section evidence boundary; this does not execute guest code."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import sys
import tarfile


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load(root, name):
    return json.loads((root / name).read_text(encoding="utf-8-sig"))


def identity(path):
    h = hashlib.sha256()
    size = 0
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            size += len(chunk)
            h.update(chunk)
    return size, h.hexdigest()


def pair(row):
    return row["bytes"], row["sha256"]


def basename(value):
    return value.replace("\\", "/").rsplit("/", 1)[-1]


def module(path):
    spec = importlib.util.spec_from_file_location("file_section_proof", path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def projections(root):
    manifest = root / "public-projections.json"
    if not manifest.exists():
        return {}
    rows = load(root, manifest.name)["files"]
    result = {}
    for row in rows:
        name = row["path"]
        require(Path(name).name == name, "projection path")
        require(name not in result, "duplicate projection")
        require(identity(root / name) == pair(row["published"]), "projection bytes")
        require(row["withheld-records"] > 0, "empty projection")
        result[name] = row
        if row["kind"] == "cargo-artifact-only":
            records = [
                json.loads(line) for line in (root / name).read_text().splitlines()
            ]
            require(records, "empty Cargo artifact projection")
            require(
                all(
                    r.get("reason")
                    in {"compiler-artifact", "build-script-executed", "build-finished"}
                    for r in records
                ),
                "private diagnostic in projection",
            )
            require(
                any(r.get("reason") == "compiler-artifact" for r in records),
                "missing artifacts",
            )
        else:
            require(
                row["kind"] == "private-build-transcript-withheld", "projection kind"
            )
    return result


def recorded(root, row, bindings, name=None):
    name = name or basename(row.get("log", row.get("path", "")))
    actual = identity(root / name)
    expected = pair(row)
    if actual != expected:
        binding = bindings.get(name)
        require(binding is not None, "unbound projected identity: " + name)
        require(actual == pair(binding["published"]), "changed projection: " + name)
        require(
            expected == pair(binding["original"]), "changed original identity: " + name
        )
    return actual


def steps(root, receipt, bindings):
    require(
        receipt["state"] == "finished" and receipt["exit-code"] == 0,
        "unfinished owning execution",
    )
    require(receipt["steps"], "empty owning selection")
    for step in receipt["steps"]:
        require(step["exit-code"] == 0, "owning child failed")
        recorded(root, step, bindings)


def stable(receipt, keys):
    for key in keys:
        require(receipt[key + "-before"] == receipt[key + "-after"], key + " changed")


def owning(root, source, bindings):
    names = {
        "macos": (
            "macos-file-section-candidate1-owning-gates.json",
            "macos-file-section-candidate1-archives-before.json",
            "macos-file-section-candidate1-archives-after.json",
        ),
        "linux": (
            "linux-file-section-candidate1-owning-gates.json",
            "linux-file-section-candidate1-archives-before.json",
            "linux-file-section-candidate1-archives-after.json",
        ),
        "windows": (
            "file-section-native-owning-candidate1.json",
            "native-file-section-candidate1-pre-cpp-archives.json",
            "native-file-section-candidate1-final-owning-archive-identities.json",
        ),
    }
    inputs = {
        r["path"]: pair(r)
        for r in load(root, "file-section-candidate1-source.json")["files"]
    }
    result = {}
    for host, (gate, before, after) in names.items():
        receipt = load(root, gate)
        steps(root, receipt, bindings)
        expected_steps = {
            "macos": [
                "prime-artifacts",
                "archives-before",
                "build",
                "ctest",
                "archives-after",
            ],
            "linux": [
                "shipping",
                "archive-before",
                "cpp",
                "artifacts",
                "archive-identities",
            ],
            "windows": [
                "toolchain",
                "artifacts-before",
                "archive-before",
                "cpp",
                "artifacts-after",
                "archive-after",
            ],
        }
        require(
            [r.get("gate", r.get("name")) for r in receipt["steps"]]
            == expected_steps[host],
            "incomplete owning gate selection",
        )
        require(
            receipt["archive-identities-stable"],
            "archive changed during owning consumers",
        )
        a, b = load(root, before), load(root, after)
        require(a["archives"] == b["archives"], "before/after archive identity")
        require(a["sources"] == b["sources"], "before/after owning source")
        require(len(b["sources"]) == 3857, "owning source selection")
        for row in b["sources"]:
            require(
                inputs.get(row["path"]) == pair(row),
                "owning source is not frozen source",
            )
        require(b["core-features"] == [], "owning core features")
        for snapshot in [a, b]:
            recorded(root, snapshot["compiler-messages"], bindings)
            messages = (
                (root / basename(snapshot["compiler-messages"]["path"]))
                .read_text()
                .splitlines()
            )
            records = []
            for line in messages:
                try:
                    records.append(json.loads(line))
                except ValueError:
                    pass
            for key, target, features in [
                ("core-artifact", "rax", []),
                ("carrier-artifact", "assist_rs", ["default", "rax"]),
            ]:
                artifact = snapshot[key]
                require(artifact in records, "actual selected Cargo artifact missing")
                require(
                    artifact["target"]["name"] == target
                    and artifact["features"] == features,
                    "artifact target/features",
                )
            require(len(snapshot["archives"]) == 2, "core/carrier archive selection")
            for row, key in zip(
                snapshot["archives"], ["carrier-artifact", "core-artifact"]
            ):
                require(
                    row["path"] in snapshot[key]["filenames"],
                    "archive not selected by actual Cargo record",
                )
                require(
                    row["structural-member-walk"] == "pass" and row["members"] > 0,
                    "incomplete archive walk",
                )
        if host == "macos":
            text = (root / "macos-file-section-candidate1-owning-ctest.log").read_text()
            require("100% tests passed out of 7" in text, "macOS owning selection")
            links = load(root, "macos-file-section-candidate1-linkage.json")
            require(
                len(links) == 6
                and all(r["carrier"] == b["archives"][0]["path"] for r in links),
                "actual Ninja carrier paths",
            )
        else:
            text = (
                root
                / (
                    "file-section-native-owning-candidate1-cpp.log"
                    if host == "windows"
                    else "linux-file-section-candidate1-owning-cpp.log"
                )
            ).read_text()
            for marker in [
                "process tool: canonical discovery and explicit RAX-disabled refusal passed",
                "RAX process: 162 checks passed",
                "embedded rax_version = 1.14.0",
                "2/2 passed",
                "PASS",
            ]:
                require(marker in text, host + " owning behavior: " + marker)
        result[host] = b["archives"]
    return result


def source_inputs(root, source):
    path = root / "file-section-candidate1-source.json"
    require(
        identity(path)[1]
        == "985b35da38a798311293a8c3c48599f107e7a358547cbf2e9499d8414291413a",
        "source ledger identity",
    )
    manifest = load(root, path.name)
    require(len(manifest["files"]) == 26884, "source input count")
    require(
        len({r["path"] for r in manifest["files"]}) == 26884, "duplicate source path"
    )
    require(
        sum(r["bytes"] for r in manifest["files"]) == manifest["total-bytes"],
        "source byte total",
    )
    require(identity(root / manifest["archive"]) == pair(manifest), "overlay identity")
    with tarfile.open(root / manifest["archive"]) as archive:
        require(
            {r.name for r in archive.getmembers()}
            == {r["path"] for r in manifest["overrides"]},
            "overlay membership",
        )
        for row in manifest["overrides"]:
            data = archive.extractfile(row["path"]).read()
            require(
                (len(data), hashlib.sha256(data).hexdigest()) == pair(row),
                "overlay member identity",
            )
    for row in manifest["files"]:
        path = source / row["path"]
        if row["kind"] == "symlink-text" and path.is_symlink():
            data = os.readlink(path).encode()
            actual = len(data), hashlib.sha256(data).hexdigest()
        else:
            actual = identity(path)
        expected = pair(row)
        if os.name == "nt" and row["path"] == ".cargo/config.toml":
            require(
                expected
                == (
                    552,
                    "a198c84574684e95de13dbb1bdb7b6c918f90fdb490c649937f697fc9621f3cd",
                ),
                "original config identity",
            )
            expected = (
                374,
                "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
            )
        require(actual == expected, "source input changed: " + row["path"])
    return manifest


def fields(text, prefix):
    return [
        dict(cell.split("=", 1) for cell in line.split()[1:])
        for line in text.splitlines()
        if line.startswith(prefix)
    ]


def native(root):
    catalog = load(root.parent, "native-producers.json")["accepted"]
    expected = {
        2: 72,
        3: 149,
        5: 272,
        6: 149,
        7: 112,
        8: 430,
        10: 3,
        11: 226,
        12: 430,
        14: 3,
        15: 3,
    }
    require(
        {r["version"]: r["roles-per-view"] for r in catalog} == expected,
        "native producer selection",
    )
    total = 0
    for row in catalog:
        require(identity(root / row["receipt"]) == pair(row), "native receipt identity")
        receipt = load(root, row["receipt"])
        steps(root, receipt, {})
        require(
            len(receipt["steps"]) == 16 and len(receipt["outcomes"]) == 4,
            "native step/view selection",
        )
        require(
            [step["name"] for step in receipt["steps"]]
            == [
                profile + suffix
                for profile in ["arm64", "x64", "x86", "x86-laa"]
                for suffix in ["-build", "-before", "", "-after"]
            ],
            "native build/run order",
        )
        stable(receipt, ["source", "runtime-modules"])
        require(len(receipt["runtime-modules-before"]) == 8, "native module set")
        require(
            identity(root / basename(receipt["source-before"]["path"]))
            == pair(receipt["source-before"]),
            "native producer source",
        )
        require(
            [r["profile"] for r in receipt["outcomes"]]
            == ["arm64", "x64", "x86", "x86-laa"],
            "native ABI selection",
        )
        observations = []
        for outcome in receipt["outcomes"]:
            require(outcome["exit-code"] == 0, "native producer failed")
            require(
                outcome["executable-before"] == outcome["executable-after"],
                "native executable changed",
            )
            profile = outcome["profile"]
            before, after = [
                load(
                    root, f'native-file-section-v{row["version"]}-{profile}-{phase}.log'
                )
                for phase in ["before", "after"]
            ]
            require(before == after, "native PE inspection changed")
            require(
                pair(before) == pair(outcome["executable-before"]),
                "native PE/executable binding",
            )
            require(
                before["profile"] == profile
                and before["machine"]
                == {"arm64": 0xAA64, "x64": 0x8664, "x86": 0x14C, "x86-laa": 0x14C}[
                    profile
                ],
                "native PE machine",
            )
            require(
                before["large-address-aware"] == (profile != "x86"), "native PE LAA"
            )
            text = (root / outcome["log"]).read_text()
            cases = fields(text, "case ")
            require(len(cases) == row["roles-per-view"], "native role count")
            require(
                [int(r["role"]) for r in cases] == list(range(row["roles-per-view"])),
                "native role sequence",
            )
            summary = fields(text, "summary ")
            require(
                len(summary) == 1
                and summary[0]["handles-before"] == summary[0]["handles-after"],
                "native resource balance",
            )
            require(int(summary[0]["cases"]) == len(cases), "native summary count")
            observations.append(cases)
            total += len(cases)
            if row["version"] == 15:
                cursors = fields(text, "cursor ")
                require(
                    len(cursors) == 3
                    and all(r["before"] == r["after"] == "7" for r in cursors),
                    "native retained cursor preservation",
                )
        # These producers print pointer-width-dependent basic buffers. Only
        # compare complete rows within matching pointer-width execution views.
        if row["version"] == 14:
            # This producer maps a copy of its own independently compiled PE.
            # ARM64 and AMD64 code sizes/RVAs therefore differ. Assert the full
            # native sharing result and each image's exact local bounds.
            for cases in observations:
                for case in cases:
                    require(
                        int(case["rva"]) + 4 <= int(case["view"])
                        and int(case["view"]) % 4096 == 0,
                        "native shared image bounds",
                    )
                    require(
                        {
                            key: case[key]
                            for key in [
                                "before",
                                "after",
                                "protection",
                                "state",
                                "type",
                            ]
                        }
                        == {
                            "before": "13579BDF",
                            "after": "2468ACE0",
                            "protection": "00000004",
                            "state": "00001000",
                            "type": "01000000",
                        },
                        "native shared image coherence",
                    )
        else:
            require(
                observations[0] == observations[1]
                and observations[2] == observations[3],
                "native paired-view mismatch",
            )
    require(total == 7396, "native total roles")
    return total


def ordinary(root, bindings, archives):
    rust = load(root, "file-section-native-ordinary-candidate1.json")
    cpp = load(root, "file-section-owning-capi-candidate1.json")
    steps(root, rust, bindings)
    steps(root, cpp, bindings)
    stable(rust, ["core", "observer", "executable", "runtime"])
    stable(
        cpp,
        [
            "carrier",
            "source",
            "adapter-inputs",
            "programs",
            "runtime",
            "native",
            "executable",
        ],
    )
    require(
        len(rust["runtime-before"]) == 8
        and rust["runtime-before"] == cpp["runtime-before"],
        "ordinary runtime set",
    )
    require(
        pair(rust["core-before"]) == pair(archives["windows"][1]),
        "ordinary core differs from owning core",
    )
    require(
        pair(cpp["carrier-before"]) == pair(archives["windows"][0]),
        "ordinary carrier differs from owning carrier",
    )
    require(
        len(rust["outcomes"]) == len(cpp["outcomes"]) == 2, "ordinary program selection"
    )
    transcript = (root / "file-section-owning-capi-candidate1-ordinary.log").read_text()
    observed = [
        json.loads(line[len("api-result ") :])
        for line in transcript.splitlines()
        if line.startswith("api-result ")
    ]
    require(observed == cpp["outcomes"], "C API declared/observed outcomes differ")
    require(
        [r["program"] for r in observed]
        == [r["path"] for r in cpp["programs-before"]]
        == [r["program-before"]["path"] for r in rust["outcomes"]],
        "ordinary selected programs differ",
    )
    for row in rust["outcomes"]:
        stable(row, ["program", "native"])
        require(row["native-before"]["exit-code"] == 0, "native control failed")
        require(
            identity(root / row["log"])[1] == row["log-sha256"],
            "ordinary Rust result identity",
        )
    smoke, whoami = [r["diagnostic"] for r in cpp["outcomes"]]
    require(
        cpp["outcomes"][0]["reason"] == 4
        and cpp["outcomes"][0]["exit-code"] == 0
        and "exited with status 0" in smoke,
        "C API smoke did not exit",
    )
    require(
        cpp["outcomes"][1]["reason"] == 5 and "SEH frame walk cyclic" in whoami,
        "C API whoami frontier",
    )
    rust_smoke = (root / rust["outcomes"][0]["log"]).read_text()
    rust_whoami = (root / rust["outcomes"][1]["log"]).read_text()
    require("Complete(Exited(0))" in rust_smoke, "Rust smoke did not exit")
    require(
        "Complete(Internal(" in rust_whoami and "SEH frame walk cyclic" in rust_whoami,
        "Rust whoami frontier",
    )
    require(
        cpp["native-before"] == [r["native-before"] for r in rust["outcomes"]],
        "ordinary native controls disagree",
    )
    for row, native in zip(cpp["outcomes"], cpp["native-before"]):
        if row["reason"] == 4:
            for channel in ["stdout", "stderr"]:
                raw = bytes.fromhex(row[channel + "-hex"])
                require(
                    len(raw) == native[channel + "-bytes"]
                    and hashlib.sha256(raw).hexdigest() == native[channel + "-sha256"],
                    "exited guest/native bytes differ",
                )
    retained = load(root, "file-section-retained-owning-candidate1.json")
    require(
        retained["state"] == "finished" and retained["exit-code"] == 0,
        "retention incomplete",
    )
    expected = {
        "core": rust["core-before"],
        "carrier": cpp["carrier-before"],
        "ordinary": rust["executable-before"],
        "capi": cpp["executable-before"],
    }
    require(
        [r["role"] for r in retained["files"]] == list(expected),
        "retained artifact set",
    )
    checked = 0
    for row in retained["files"]:
        require(
            pair(row) == pair(expected[row["role"]])
            and row["source"] == expected[row["role"]]["path"],
            "retained artifact is not exercised artifact",
        )
        if os.name == "nt":
            require(
                identity(Path(row["retained"])) == pair(row),
                "retained native artifact changed",
            )
            require(
                not Path(row["retained"]).samefile(row["source"]),
                "retention is a hardlink",
            )
            checked += 1
    return {
        "smoke": "Exited(0) through owning core and C API",
        "whoami": "SEH frame-walk boundary through both owning routes; no process completion",
        "retained-native-files-hashed": checked,
    }


def regression(root, source):
    receipt = load(root, "file-section-red-test-body-binding.json")
    require(
        receipt["state"] == "finished" and receipt["expected-test-exit-code"] == 101,
        "red execution state",
    )
    require(identity(root / receipt["log"]) == pair(receipt), "red transcript identity")
    text = (root / receipt["log"]).read_text()
    require(
        "0 passed; 1 failed;" in text
        and "NtCreateSection for a file-backed section" in text,
        "red failure boundary",
    )
    binding = receipt["test-body-binding"]
    require(
        identity(root / binding["baseline-source-container"])[1]
        == binding["baseline-container-sha256"],
        "red source container",
    )
    with tarfile.open(root / binding["baseline-source-container"]) as archive:
        old = archive.extractfile(binding["candidate-source-file"]).read()
    current = (source / binding["candidate-source-file"]).read_bytes()
    marker = b"#[test]\nfn native_section_file_image_create_publishes_a_typed_owned_section_all_abis()"
    a, b = old[old.index(marker) :], current[current.index(marker) :]
    require(
        a == b and (len(a), hashlib.sha256(a).hexdigest()) == pair(binding),
        "red/green constructor body changed",
    )


def gates(root, bindings):
    for host, passed in [("macos", 8135), ("linux", 8129)]:
        receipt = load(root, f"file-section-{host}-candidate1-gates.json")
        steps(root, receipt, bindings)
        require(
            [r["name"] for r in receipt["steps"]]
            == ["focus", "full", "capi", "integration", "all-targets"],
            "Unix gate selection",
        )
        expectations = {"focus": 7, "full": passed, "capi": 183, "integration": 544}
        for step in receipt["steps"]:
            if step["name"] in expectations:
                require(
                    any(
                        f'{expectations[step["name"]]} passed; 0 failed;' in result
                        for result in step["results"]
                    ),
                    "Unix gate counts",
                )
            if step["name"] == "full":
                require(
                    any(
                        "2 ignored; 0 measured; 0 filtered out;" in result
                        for result in step["results"]
                    ),
                    "unfiltered Unix selection",
                )
    # Native gate aggregation and actual owning ordinary routes are bound by
    # their final execution receipts, added after those operations complete.
    receipt = load(root, "file-section-candidate1-windows.json")
    audit = load(root, "file-section-candidate1-final-gates.json")
    require(
        audit["state"] == "verified" and audit["source-inputs"] == 26884,
        "native final gate audit",
    )
    recorded(root, audit["source-receipt"], bindings)
    require(
        audit["required-executed-steps"]
        == [
            "toolchain",
            "clean-core",
            "test-build",
            "file-focus",
            "section-focus",
            "full",
            "capi",
            "integration",
            "all-targets",
        ],
        "native audited gate selection",
    )
    require(
        receipt["state"] == "failed"
        and receipt["current-step"] == "core-build"
        and audit["extra-core-artifact"]["fresh"]
        and 'not row["fresh"]' in receipt["error"],
        "unclassified native driver failure",
    )
    require(receipt["steps"], "native gate selection")
    names = [r["name"] for r in receipt["steps"]]
    require(
        names[:9]
        == [
            "toolchain",
            "clean-core",
            "test-build",
            "file-focus",
            "section-focus",
            "full",
            "capi",
            "integration",
            "all-targets",
        ],
        "native gate order",
    )
    for step in receipt["steps"][:9]:
        recorded(root, step, bindings)
        require(
            step["exit-code"] == (101 if step["name"] == "full" else 0),
            "native gate exit",
        )
    full = receipt["steps"][5]
    require(
        any(
            "7598 passed; 5 failed; 2 ignored; 0 measured; 0 filtered out;" in result
            for result in full["results"]
        ),
        "native full count",
    )
    text = (root / full["log"]).read_text()
    actual = set(
        re.findall(r"^    (\S+)\r?$", text[text.rfind("\nfailures:\n") :], re.M)
    )
    expected = set(load(root, "native-existing-failures.json")["failures"])
    require(
        actual == expected == set(receipt["exact-existing-failures"]),
        "native failure set",
    )
    require(
        receipt["test-image-before"] == receipt["test-image-after"],
        "native executed test image changed",
    )
    require(
        audit["live-test-image-before"]
        == audit["live-test-image-after"]
        == receipt["test-image-before"],
        "native post-gate image audit",
    )
    artifact = receipt["test-artifact"]
    require(
        not artifact["fresh"]
        and artifact["features"] == []
        and artifact["profile"]["test"],
        "native primary not rebuilt",
    )
    require(
        artifact["executable"] == receipt["test-image-before"]["path"],
        "native selected executable",
    )
    for name, count in [
        ("file-focus", 8),
        ("section-focus", 25),
        ("capi", 183),
        ("integration", 4),
    ]:
        step = next(r for r in receipt["steps"] if r["name"] == name)
        require(
            any(f"{count} passed; 0 failed;" in result for result in step["results"]),
            "native focused count",
        )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument(
        "--records", type=Path, default=Path(__file__).parent / "validation"
    )
    parser.add_argument(
        "--native-only", action="store_true", help="pre-publication partial audit only"
    )
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    root = args.records
    native_rows = native(root)
    source_inputs(root, args.source_root)
    regression(root, args.source_root)
    bindings = projections(root)
    if not args.native_only:
        gates(root, bindings)
        archives = owning(root, args.source_root, bindings)
        behavior = ordinary(root, bindings, archives)
        for host in ["macos", "linux", "windows"]:
            for kind in ["oracle", "owning"]:
                stress = load(
                    root, f"file-section-{kind}-stress-{host}-candidate1.json"
                )
                require(
                    stress["negative-rejections"]
                    == stress["positive-restorations"]
                    == 8
                    and len(stress["cases"]) == 8
                    and all(
                        r["rejected"] and r["fresh-positive-restoration"]
                        for r in stress["cases"]
                    ),
                    "incomplete adversarial evidence checks",
                )
                if kind == "owning":
                    reasons = {r["case"]: r["reason"] for r in stress["cases"]}
                    require(
                        reasons["changed-owning-source-set"]
                        == "owning source selection",
                        "source-set corruption stopped before semantic check",
                    )
                    require(
                        reasons["incomplete-archive-walk"] == "incomplete archive walk",
                        "archive-walk corruption stopped before semantic check",
                    )
    result = {
        "state": "pre-publication-partial" if args.native_only else "verified",
        "native-rows": native_rows,
        "source-inputs": 26884,
        "scope": "Native controls, frozen source and red/green body only; not final owning/full-gate validation.",
    }
    if not args.native_only:
        result["ordinary"] = behavior
        result["scope"] = (
            "Frozen source, complete gates, selected owning archives/consumers and actual ordinary routes; no full current-root GUI/package or full-userland completion claim."
        )
    if args.output:
        args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
