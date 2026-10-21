"""Audit the file-open evidence boundary; this does not execute guest code."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import sys


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
    spec = importlib.util.spec_from_file_location("file_open_proof", path)
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
            "macos-file-open-epoch2-owning-gates.json",
            "macos-file-open-epoch2-archives-before.json",
            "macos-file-open-epoch2-archives-after.json",
        ),
        "linux": (
            "linux-file-open-epoch2-owning-gates.json",
            "linux-file-open-epoch2-archives-before.json",
            "linux-file-open-epoch2-archives-after.json",
        ),
        "windows": (
            "file-open-native-owning-epoch2.json",
            "native-file-open-epoch2-pre-cpp-archives.json",
            "native-file-open-epoch2-final-owning-archive-identities.json",
        ),
    }
    inputs = {
        r["path"]: pair(r) for r in load(root, "file-open-source-epoch2.json")["files"]
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
        require(len(b["sources"]) == 3849, "owning source selection")
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
            text = (root / "macos-file-open-epoch2-owning-ctest.log").read_text()
            require("100% tests passed out of 7" in text, "macOS owning selection")
            links = load(root, "macos-file-open-epoch2-linkage.json")
            require(
                len(links) == 6
                and all(r["carrier"] == b["archives"][0]["path"] for r in links),
                "actual Ninja carrier paths",
            )
        else:
            text = (
                root
                / (
                    "file-open-native-owning-epoch2-cpp.log"
                    if host == "windows"
                    else "linux-file-open-epoch2-owning-cpp.log"
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


def ordinary(root, bindings, archives):
    rust = load(root, "file-open-native-ordinary-epoch2.json")
    cpp = load(root, "file-open-owning-capi-epoch2.json")
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
    transcript = (root / "file-open-owning-capi-epoch2-ordinary.log").read_text()
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
        cpp["outcomes"][1]["reason"] == 5
        and "NtCreateSection" in whoami
        and "file-backed" in whoami,
        "C API whoami frontier",
    )
    rust_smoke = (root / rust["outcomes"][0]["log"]).read_text()
    rust_whoami = (root / rust["outcomes"][1]["log"]).read_text()
    require("Complete(Exited(0))" in rust_smoke, "Rust smoke did not exit")
    require(
        "Complete(Internal(" in rust_whoami
        and "NtCreateSection" in rust_whoami
        and "file-backed" in rust_whoami,
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
    retained = load(root, "file-open-retained-owning-epoch2.json")
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
        "whoami": "NtCreateSection/file-backed explicit frontier through both routes",
        "retained-native-files-hashed": checked,
    }


def regression(root, source, bindings):
    receipt = load(root, "file-open-delete-baseline-unit-v3.json")
    require(
        receipt["state"] == "finished" and receipt["exit-code"] == 0,
        "compiled baseline incomplete",
    )
    row = receipt["steps"][0]
    require(row["exit-code"] == 101, "compiled pre-fix test must fail")
    recorded(root, row, bindings)
    text = (root / row["log"]).read_text()
    require(
        "0 passed; 1 failed;" in text and "code: 2, kind: NotFound" in text,
        "expected pre-fix failure absent",
    )
    require(
        identity(source / "src/user/windows/dll/files/tests.rs")
        == pair(receipt["override-after"]),
        "red/green regression source differs",
    )


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--source-root", type=Path, required=True)
    p.add_argument("--records", type=Path, default=Path(__file__).parent / "validation")
    p.add_argument("--output", type=Path)
    p.add_argument(
        "--allow-pending-windows",
        action="store_true",
        help="pre-publication inspection only; omits complete three-host gates",
    )
    args = p.parse_args()
    root, source = args.records, args.source_root
    proof = module(root / "verify-file-open-evidence.py")
    saved = sys.argv
    sys.argv = [
        "verify-file-open-evidence",
        "--records",
        str(root),
        "--source",
        str(source),
    ]
    if not args.allow_pending_windows:
        sys.argv += ["--require-gates"]
    try:
        proof.main()
    finally:
        sys.argv = saved
    bindings = projections(root)
    archives = owning(root, source, bindings)
    behavior = ordinary(root, bindings, archives)
    regression(root, source, bindings)
    if not args.allow_pending_windows:
        receipt = load(root, "file-open-native-epoch2-isolated-gates.json")
        log = next(r["log"] for r in receipt["steps"] if r["name"] == "full")
        observed = sorted(
            re.findall(r"^    (smir::[^\r\n]+)$", (root / log).read_text(), re.M)
        )
        require(
            observed == sorted(receipt["exact-existing-failures"]),
            "observed Windows failure set differs",
        )
    for host in ["macos", "linux", "windows"]:
        stress = load(root, "file-open-oracle-stress-" + host + "-epoch2.json")
        require(
            len(stress["cases"]) == 8
            and all(
                r["rejected"] and r["positive-restoration"] for r in stress["cases"]
            ),
            "oracle stress incomplete",
        )
        if not args.allow_pending_windows:
            extra = load(
                root, "file-open-owning-stress-" + host + "-epoch2-round2.json"
            )
            require(
                extra["negative-rejections"] == extra["positive-restorations"] == 8
                and len(extra["cases"]) == 8
                and all(
                    r["rejected"] and r["fresh-positive-restoration"]
                    for r in extra["cases"]
                ),
                "owning stress incomplete",
            )
            reasons = {r["case"]: r["reason"] for r in extra["cases"]}
            require(
                reasons["changed-owning-source-set"] == "owning source selection",
                "source-selection control stopped before semantic check",
            )
            require(
                reasons["incomplete-archive-walk"] == "incomplete archive walk",
                "archive-walk control stopped before semantic check",
            )
    result = {
        "state": (
            "pre-publication-partial" if args.allow_pending_windows else "verified"
        ),
        "owning-hosts": list(archives),
        "public-projections": len(bindings),
        "ordinary": behavior,
        "scope": "Recorded native and compiled behavior, exact frozen source, actual selected core/carrier archives, retained ABI184 consumers. No full current-root GUI/package or complete process-userland claim.",
    }
    if args.output:
        args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
