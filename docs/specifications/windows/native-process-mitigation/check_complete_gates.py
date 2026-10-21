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
        ("macos", 8157, 0, 544),
        ("linux", 8151, 0, 544),
        ("windows", 7623, 5, 4),
    ]:
        name = (
            "process-mitigation-candidate2-windows-retry2.json"
            if host == "windows"
            else "process-mitigation-" + host + "-candidate2-gates.json"
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
            require(receipt["source-inputs"] == 26913, "native source count")
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
    manifest = read("process-mitigation-owning-source-hashes-candidate2.json")
    require(len(manifest["sources"]) == 3886, "owning source extent")
    owning = []
    for host in ["macos", "linux", "windows"]:
        if host == "windows":
            receipt = read("process-mitigation-native-owning-candidate2-retry2.json")
            require(
                receipt["state"] == "finished" and receipt["exit-code"] == 0,
                "native owning gate",
            )
            before = read("native-process-mitigation-candidate2-pre-cpp-archives.json")
            after = read(
                "native-process-mitigation-candidate2-final-owning-archive-identities.json"
            )
        else:
            receipt = read(host + "-process-mitigation-candidate2-owning-gates.json")
            before = read(host + "-process-mitigation-candidate2-archives-before.json")
            after = read(host + "-process-mitigation-candidate2-archives-after.json")
            require(
                receipt["state"] == "finished" and receipt["exit-code"] == 0,
                "owning result",
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
    ordinary = read("process-mitigation-native-ordinary-candidate2-retry2.json")
    capi = read("process-mitigation-owning-capi-candidate2-retry2.json")
    for r in [ordinary, capi]:
        require(
            r["state"] == "finished" and r["exit-code"] == 0, "ordinary run incomplete"
        )
        for row in r["steps"]:
            log(row)
    rust = (
        directory / "process-mitigation-native-ordinary-candidate2-retry2-whoami.log"
    ).read_text()
    require(
        "terminal-call=2816 result=Complete(Internal(" in rust
        and "NtAlpcSendWaitReceivePort" in rust,
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
        and whoami["turns-started"] == 2817
        and "NtAlpcSendWaitReceivePort" in whoami["diagnostic"],
        "C API failure classification",
    )
    final = read("process-mitigation-final-native-audit-v2.json")
    require(
        final["state"] == "verified"
        and len(final["runtime-module-union"]) == 10
        and len(final["retained-current-artifacts"]) == 4,
        "final native source/runtime/artifact union",
    )
    cross = read("process-mitigation-cross-final.json")
    require(
        cross["state"] == "verified" and len(cross["targets"]) == 2, "cross targets"
    )
    require(
        "Finished `dev`"
        in (directory / "process-mitigation-clippy-linux-canonical.log").read_text(),
        "Clippy completion",
    )
    links = read("macos-process-mitigation-candidate2-linkage.json")
    require(
        len(links["targets"]) == 8
        and all(x["selected-carrier-in-link-rule"] for x in links["targets"]),
        "actual carrier links",
    )
    require(
        "100% tests passed out of 7"
        in (
            directory / "macos-process-mitigation-candidate2-owning-ctest.log"
        ).read_text(),
        "seven CTests",
    )
    return {
        "state": "verified",
        "suites": suite,
        "owning": owning,
        "macos-carrier-links": 8,
        "macos-ctests": 7,
        "native-runtime-DLL-union": 10,
        "ordinary-new-boundary": "NtAlpcSendWaitReceivePort",
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
