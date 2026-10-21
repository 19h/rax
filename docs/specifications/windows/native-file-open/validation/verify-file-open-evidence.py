"""Verify native controls and observed validation outcomes, without compiling code."""

import argparse
import hashlib
import json
import re
import os
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

PROFILES = ("arm64", "x64", "x86", "x86-laa")
SOURCE_SHA = "f257d4a412aeed642d4351ca6e7d1dea89ec95173a09eb430e5036651ac625a5"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load(root, name):
    return json.loads((root / name).read_text(encoding="utf-8-sig"))


def fingerprint(path):
    raw = path.read_bytes()
    return len(raw), hashlib.sha256(raw).hexdigest()


def native(root, version, roles):
    prefix = "native-file-open-" + version
    receipt = load(root, prefix + "-launch.json")
    require(receipt["state"] == "finished" and receipt["exit-code"] == 0, prefix)
    require(
        receipt["source-before"] == receipt["source-after"], prefix + " source changed"
    )
    require(
        receipt["runtime-modules-before"] == receipt["runtime-modules-after"],
        prefix + " runtime changed",
    )
    identity = receipt["source-before"]
    require(
        fingerprint(root / (prefix + ".cpp"))
        == (identity["bytes"], identity["sha256"]),
        "native producer identity",
    )
    outcomes = receipt["outcomes"]
    require([r["profile"] for r in outcomes] == list(PROFILES), prefix + " profile set")
    steps = receipt["steps"]
    require(
        len(steps) == 16 and all(r["exit-code"] == 0 for r in steps),
        prefix + " native steps",
    )
    results = {}
    for outcome in outcomes:
        profile = outcome["profile"]
        require(outcome["exit-code"] == 0, prefix + "/" + profile)
        require(
            outcome["executable-before"] == outcome["executable-after"],
            "native executable changed",
        )
        name = prefix + "-" + profile + ".log"
        row = next(r for r in steps if r["name"] == profile)
        require(
            fingerprint(root / name) == (row["bytes"], row["sha256"]),
            "native transcript identity",
        )
        cases = {}
        for line in (root / name).read_text().splitlines():
            if line.startswith("case "):
                values = dict(part.split("=", 1) for part in line.split()[1:])
                role = int(values["role"])
                require(role not in cases, "duplicate native role")
                cases[role] = values
        require(set(cases) == set(roles), prefix + "/" + profile + " native role set")
        results[profile] = cases
    return results


def normalized(row, width, paired):
    value = (
        None
        if row["hout"] == "-"
        else int.from_bytes(bytes.fromhex(row["hout"]), "little")
    )
    handle = (
        "-"
        if value is None
        else (
            "zero"
            if value == 0
            else (
                "sentinel"
                if value == int.from_bytes(b"\xa5" * width, "little")
                else "published"
            )
        )
    )
    result = {
        "status": int(row["status"], 16),
        "hprot": int(row["hprot"], 16),
        "iprot": int(row["iprot"], 16),
        "iosb": row["iosb"],
    }
    result["handle" if paired else "hout"] = handle
    if paired:
        result["valid"] = row["hvalid"] == "1"
    return result


def oracles(root, source):
    v7 = native(root, "v7", range(1, 404))
    v10 = native(root, "v10", range(1, 460))
    sharing = native(root, "sharing-v2", range(4))
    for profile, cases in sharing.items():
        for role, row in cases.items():
            require(
                int(row["status"], 16) == (0 if role == 0 else 0xC0000043),
                "independent deletion-sharing result",
            )
            require(
                row["exists-after-close"] == str(int(role != 0)),
                "independent deletion side effect",
            )
    directory = (
        source / "src/user/windows/process/sched/tests/services_tests/file_open_tests"
    )
    pairs = json.loads((directory / "paired-v7.json").read_text())["cases"]
    require(len(pairs) == 210, "pair count")
    require(
        [(r["left"], r["right"]) for r in pairs]
        == [(a, b) for a in range(21) for b in range(a + 1, 21)],
        "pair coverage",
    )
    boundaries = json.loads((directory / "boundaries-v10.json").read_text())["cases"]
    require(
        [r["role"] for r in boundaries] == [*range(404, 426), *range(428, 460)],
        "boundary role set",
    )
    for dataset, rows, paired in [(v7, pairs, True), (v10, boundaries, False)]:
        for row in rows:
            role = row["role"]
            for profile, key, width in [
                ("arm64", "native", 8),
                ("x64", "native", 8),
                ("x86", "wow", 4),
                ("x86-laa", "wow", 4),
            ]:
                require(
                    normalized(dataset[profile][role], width, paired) == row[key],
                    "independent oracle mismatch " + str(role) + "/" + profile,
                )
    return {
        "independent-v10-calls": 1836,
        "independent-sharing-calls": 16,
        "compiled-pair-calls-per-host": 630,
        "compiled-boundary-calls-per-host": 162,
    }


def gates(root, host):
    name = (
        "file-open-native-epoch2-isolated-gates.json"
        if host == "windows"
        else "file-open-" + host + "-epoch2-gates.json"
    )
    receipt = load(root, name)
    require(
        receipt["state"] == "finished" and receipt["exit-code"] == 0,
        host + " incomplete gates",
    )
    steps = {row["name"]: row for row in receipt["steps"]}
    require(
        set(steps)
        == ({"toolchain", "clean-core", "test-build"} if host == "windows" else set())
        | {"focus", "full", "capi", "integration", "all-targets"},
        "gate set",
    )
    expected = {
        "focus": 26,
        "full": {"macos": 8128, "linux": 8122, "windows": 7590}[host],
        "capi": 183,
        "integration": 4 if host == "windows" else 544,
    }
    for gate, step in steps.items():
        code = 101 if gate == "full" and host == "windows" else 0
        require(step["exit-code"] == code, host + "/" + gate + " exit")
        path = root / step["log"]
        require(
            fingerprint(path) == (step["bytes"], step["sha256"]),
            "gate transcript identity",
        )
        observed = re.findall(
            r"test result: ([^\r\n]+)", path.read_text(errors="replace")
        )
        require(observed == step["results"], "declared/observed test summaries differ")
        if gate in expected:
            failed = 5 if gate == "full" and host == "windows" else 0
            marker = str(expected[gate]) + " passed; " + str(failed) + " failed;"
            require(
                any(marker in result for result in step["results"]),
                host + "/" + gate + " observed count",
            )
        if gate == "full":
            require(
                any(
                    "2 ignored; 0 measured; 0 filtered out;" in result
                    for result in step["results"]
                ),
                "unfiltered full suite required",
            )
    if host == "windows":
        prior = load(root, "file-open-native-epoch1-gates.json")
        require(
            receipt["exact-existing-failures"] == prior["exact-existing-failures"],
            "new Windows failure set",
        )
        selected = receipt["selected-test-artifact"]
        build = (root / steps["test-build"]["log"]).read_text()
        artifacts = []
        for line in build.splitlines():
            try:
                artifacts.append(json.loads(line))
            except ValueError:
                pass
        require(selected in artifacts, "selected test artifact not emitted by Cargo")
        require(selected["fresh"] is False, "library test was not freshly rebuilt")
        require(
            selected["target"]["name"] == "rax"
            and selected["features"] == []
            and selected["profile"]["test"],
            "selected library test target",
        )
        image = receipt["selected-test-image"]
        require(selected["executable"] == image["path"], "selected test image path")
        require(
            image["sha256"]
            != receipt["retained-rejected-full-image"]["retained"]["sha256"],
            "fresh candidate still selects the rejected baseline image",
        )
        for gate in ["focus", "full"]:
            require(
                steps[gate]["argv"][0] == image["path"]
                and steps[gate]["test-image-before"] == image
                and steps[gate]["test-image-after"] == image,
                "test image changed between build and execution",
            )
    return expected


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--records", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--require-gates", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    require(
        fingerprint(args.records / "file-open-source-epoch2.json")[1] == SOURCE_SHA,
        "frozen source ledger identity",
    )
    manifest = load(args.records, "file-open-source-epoch2.json")

    def source_file(row):
        path = args.source / row["path"]
        raw = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
        expected = (row["bytes"], row["sha256"])
        if row["path"] == ".cargo/config.toml" and os.name == "nt":
            require(
                expected
                == (
                    552,
                    "a198c84574684e95de13dbb1bdb7b6c918f90fdb490c649937f697fc9621f3cd",
                ),
                "original Cargo ledger changed",
            )
            expected = (
                374,
                "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
            )
        require(
            (len(raw), hashlib.sha256(raw).hexdigest()) == expected,
            "actual source differs: " + row["path"],
        )

    with ThreadPoolExecutor(max_workers=4) as workers:
        list(workers.map(source_file, manifest["files"]))
    result = oracles(args.records, args.source)
    if args.require_gates:
        result["gates"] = {
            host: gates(args.records, host) for host in ["macos", "linux", "windows"]
        }
    result["scope"] = (
        "Independent native controls, compiled oracle inputs and requested observed gate outcomes. Digests bind records; behavior rests on native calls and compiled executions."
    )
    if args.output:
        args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
