"""Audit the complete captured source/consumer gates without an all-green CI claim."""

from pathlib import Path
import argparse
import hashlib
import json


def require(ok, what):
    if not ok:
        raise ValueError(what)


def check(directory):
    def read(name):
        return json.loads((directory / name).read_text())

    projected = {x["path"]: x for x in read("public-projections.json")["files"]}

    def log(row):
        name = row["log"]
        data = (directory / name).read_bytes()
        expected = {"bytes": row["bytes"], "sha256": row["sha256"]}
        actual = {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
        if name in projected:
            require(projected[name]["original"] == expected, "projection original")
            expected = projected[name]["published"]
        require(actual == expected, "captured log: " + name)
        return data.decode(errors="replace")

    suite = []
    for host, passed, failures, integration in [
        ("macos", 10321, 0, 544),
        ("linux", 8134, 0, 544),
        ("windows", 7603, 5, 4),
    ]:
        name = (
            "current-el-candidate1-windows.json"
            if host == "windows"
            else "current-el-" + host + "-candidate1-gates.json"
        )
        receipt = read(name)
        require(
            receipt["state"] == "finished" and receipt["exit-code"] == 0,
            "unfinished " + host,
        )
        steps = {x["name"]: x for x in receipt["steps"]}
        for row in receipt["steps"]:
            log(row)
        for gate in ["full", "capi", "integration", "all-targets"]:
            require(gate in steps, "missing " + host + " " + gate)
            require(
                steps[gate]["exit-code"]
                == (101 if host == "windows" and gate == "full" else 0),
                "gate result",
            )
        for gate, count in [
            ("full", passed),
            ("capi", 183),
            ("integration", integration),
        ]:
            expected = f'{count} passed; {failures if gate=="full"else 0} failed;'
            require(
                any(expected in x for x in steps[gate]["results"]),
                "test count " + host + " " + gate,
            )
        require(
            any(
                "2 ignored; 0 measured; 0 filtered out;" in x
                for x in steps["full"]["results"]
            ),
            "unfiltered full suite",
        )
        if host == "windows":
            require(receipt["source-inputs"] == 26885, "native source count")
            require(
                receipt["test-image-before"] == receipt["test-image-after"],
                "native executable identity",
            )
            known = [
                "bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry",
                "bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry",
                "bit::lowers_bzhi_x_two_imms_with_flags_sets_carry",
                "bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards",
                "vector::lowers_vector_fp16_arithmetic_runtime",
            ]
            require(
                receipt["exact-existing-failures"]
                == ["smir::lower::aarch64::tests::" + x for x in known],
                "unclassified native failures",
            )
        suite.append(
            {
                "host": host,
                "passed": passed,
                "known-failures": failures,
                "ignored": 2,
                "capi-passed": 183,
                "integration-passed": integration,
            }
        )
    manifest = read("current-el-owning-source-hashes-candidate1.json")
    require(len(manifest["sources"]) == 3858, "owning source extent")
    owning = []
    for host in ["macos", "linux", "windows"]:
        if host == "windows":
            receipt = read("current-el-native-owning-candidate1.json")
            require(
                receipt["state"] == "finished" and receipt["exit-code"] == 0,
                "native owning gate",
            )
            before = read("native-current-el-candidate1-pre-cpp-archives.json")
            after = read(
                "native-current-el-candidate1-final-owning-archive-identities.json"
            )
        else:
            receipt = read(host + "-current-el-candidate1-owning-gates.json")
            before = read(host + "-current-el-candidate1-archives-before.json")
            after = read(host + "-current-el-candidate1-archives-after.json")
            if host == "linux":
                require(
                    receipt["state"] == "failed"
                    and receipt["steps"][-1]["gate"] == "cpp",
                    "retained original launcher failure",
                )
                require(
                    "No such file" in log(receipt["steps"][-1]),
                    "launcher failure classification",
                )
                resume = read("linux-current-el-candidate1-owning-retry1-gates.json")
                require(
                    resume["state"] == "finished" and resume["exit-code"] == 0,
                    "Linux resume",
                )
                require(
                    [x["gate"] for x in resume["steps"]]
                    == ["cpp", "artifacts", "archive-identities"],
                    "Linux resumed gates",
                )
                require(
                    all(
                        x["exit-code"] == 0
                        for x in receipt["steps"][:2] + resume["steps"]
                    ),
                    "Linux complete owning stages",
                )
                for row in resume["steps"]:
                    log(row)
            else:
                require(
                    receipt["state"] == "finished" and receipt["exit-code"] == 0,
                    "Mac owning result",
                )
        for row in receipt["steps"]:
            log(row)
        require(
            before["archives"] == after["archives"] and len(after["archives"]) == 2,
            "stable owning archive pair",
        )
        require(
            before["sources"] == after["sources"] == manifest["sources"],
            "owning source set",
        )
        require(after["core-features"] == [], "embedded root feature isolation")
        for row in after["archives"]:
            require(
                row["members"] > 0 and row["structural-member-walk"] == "pass",
                "complete archive walk",
            )
        owning.append({"host": host, "verified": True, "archives": 2})
    ordinary = read("current-el-native-ordinary-candidate1.json")
    capi = read("current-el-owning-capi-candidate1.json")
    for r in [ordinary, capi]:
        require(
            r["state"] == "finished" and r["exit-code"] == 0, "ordinary run incomplete"
        )
        for row in r["steps"]:
            log(row)
    rust = (directory / "current-el-native-ordinary-candidate1-whoami.log").read_text()
    require(
        "terminal-call=2550 result=Complete(Internal(" in rust
        and "NtQueryVolumeInformationFile" in rust,
        "Rust new boundary",
    )
    smoke, whoami = capi["outcomes"]
    require(
        smoke["reason"] == 4
        and smoke["exit-code"] == 0
        and smoke["turns-started"] == 975,
        "smoke exit",
    )
    require(
        whoami["reason"] == 5
        and whoami["exit-code"] == 0
        and whoami["turns-started"] == 2551
        and "NtQueryVolumeInformationFile" in whoami["diagnostic"],
        "C API failure classification",
    )
    final = read("current-el-final-native-audit-v1.json")
    require(
        final["state"] == "verified"
        and len(final["runtime-module-union"]) == 10
        and len(final["retained-current-artifacts"]) == 4,
        "final native source/runtime/artifact union",
    )
    cross = read("current-el-cross-final.json")
    require(
        cross["state"] == "verified" and len(cross["targets"]) == 2, "cross targets"
    )
    require(
        "Finished `dev`" in (directory / "current-el-clippy-linux.log").read_text(),
        "Clippy completion",
    )
    links = read("macos-current-el-candidate1-linkage.json")
    require(
        len(links["targets"]) == 8
        and all(x["selected-carrier-in-link-rule"] for x in links["targets"]),
        "actual carrier links",
    )
    require(
        "100% tests passed out of 7"
        in (directory / "macos-current-el-candidate1-owning-ctest.log").read_text(),
        "seven CTests",
    )
    return {
        "state": "verified",
        "suites": suite,
        "owning": owning,
        "macos-carrier-links": 8,
        "macos-ctests": 7,
        "native-runtime-DLL-union": 10,
        "ordinary-new-boundary": "NtQueryVolumeInformationFile",
        "whole-root-and-CI-success": False,
        "scope": "Retained Assist ABI184 consumers; current ABI274 root packages/GUI and CI are separate evidence.",
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--directory", type=Path, default=Path(__file__).parent / "validation"
    )
    args = parser.parse_args()
    print(json.dumps(check(args.directory), indent=2))


if __name__ == "__main__":
    main()
