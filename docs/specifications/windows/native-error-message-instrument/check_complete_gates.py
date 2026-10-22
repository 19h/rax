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
        ("macos", 8172, 0, 544),
        ("linux", 8166, 0, 544),
        ("windows", 7641, 5, 4),
    ]:
        name = (
            "eri-candidate1-windows-retry2.json"
            if host == "windows"
            else "eri-"
            + host
            + "-candidate1"
            + ("-retry1" if host == "macos" else "")
            + "-gates.json"
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
            require(receipt["source-inputs"] == 28816, "native source count")
            prior = read("eri-candidate1-windows-retry1.json")
            raw_prior = (directory / "eri-candidate1-windows-retry1.json").read_bytes()
            require(
                receipt["continued-from"]["sha256"]
                == hashlib.sha256(raw_prior).hexdigest(),
                "continued receipt identity",
            )
            require(
                prior["state"] == "failed" and prior["current-step"] == "integration",
                "original PDB setup failure",
            )
            inherited = [x for x in prior["steps"] if x["name"] != "integration"]
            require(
                receipt["steps"][: len(inherited)] == inherited,
                "unchanged classified full/CAPI/focused evidence",
            )
            require(
                receipt["overlay"]
                == prior["overlay"]
                == read("eri-candidate1-source.json"),
                "same frozen source for resumed gates",
            )

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
    manifest = read("eri-owning-source-hashes-candidate1.json")
    require(len(manifest["sources"]) == 3962, "owning source extent")
    owning = []
    for host in ["macos", "linux", "windows"]:
        if host == "windows":
            receipt = read("eri-native-owning-candidate1.json")
            require(
                receipt["state"] == "finished" and receipt["exit-code"] == 0,
                "native owning gate",
            )
            before = read("native-eri-candidate1-pre-cpp-archives.json")
            after = read("native-eri-candidate1-final-owning-archive-identities.json")
        else:
            receipt = read(host + "-eri-candidate1-owning-gates.json")
            before = read(host + "-eri-candidate1-archives-before.json")
            after = read(host + "-eri-candidate1-archives-after.json")
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
    ordinary = read("eri-native-ordinary-candidate1.json")
    capi = read("eri-owning-capi-candidate1.json")
    for r in [ordinary, capi]:
        require(
            r["state"] == "finished" and r["exit-code"] == 0, "ordinary run incomplete"
        )
        for row in r["steps"]:
            log(row)
    rust = (directory / "eri-native-ordinary-candidate1-whoami.log").read_text()
    require(
        "terminal-call=2832 result=Complete(Internal(" in rust and "NtOpenKey" in rust,
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
        and whoami["turns-started"] == 2833
        and "NtOpenKey" in whoami["diagnostic"],
        "C API failure classification",
    )
    final = read("eri-final-native-audit-v3.json")
    require(
        final["state"] == "verified"
        and len(final["runtime-module-union"]) == 10
        and len(final["retained-current-artifacts"]) == 4,
        "final native source/runtime/artifact union",
    )
    cross = read("eri-cross-candidate1.json")
    require(
        cross["state"] == "finished"
        and cross["exit-code"] == 0
        and len(cross["targets"]) == 2,
        "cross targets",
    )
    require(
        "Finished `dev`"
        in (directory / "eri-clippy-linux-canonical-v1.log").read_text(),
        "Clippy completion",
    )
    for target in cross["targets"]:
        require(
            target["exit-code"] == 0 and target["primary-package-cleaned"],
            "fresh cross compile",
        )
        log(target)
    source = read("eri-candidate1-source.json")
    require(len(source["files"]) == 28816, "frozen source extent")
    import tarfile

    override = read("eri-final-compiled-overrides-v1.json")
    require(len(override["overrides"]) == 25, "compiled override extent")
    log(dict(override, log=override["archive"]))
    with tarfile.open(directory / override["archive"], "r:gz") as archive:
        members = {x.name: x for x in archive.getmembers()}
        require(
            set(members) == {x["path"] for x in override["overrides"]},
            "source archive members",
        )
        for row in override["overrides"]:
            raw = archive.extractfile(members[row["path"]]).read()
            require(
                len(raw) == row["bytes"]
                and hashlib.sha256(raw).hexdigest() == row["sha256"],
                "source archive identity",
            )
    red = read("eri-native-red2-retry1.json")
    require(
        red["state"] == "finished"
        and red["exit-code"] == 0
        and red["observed-test-exit"] == 101,
        "native baseline regression",
    )
    for step in red["steps"]:
        log(step)
    require(
        "0 passed; 1 failed;"
        in log(next(x for x in red["steps"] if x["name"] == "red")),
        "native pre-fix failure",
    )
    stress = read("eri-native-proof-stress-v3.json")
    require(
        stress["state"] == "finished" and len(stress["cases"]) == 3, "stress extent"
    )
    for case in stress["cases"]:
        require(
            case["rejected"]
            and case["positive-before"]["state"]
            == case["fresh-restore"]["state"]
            == "verified",
            "negative proof and positive restore",
        )
    unix = read("eri-unix-native-runtime-current-v1.json")
    require(
        unix["state"] == "verified" and len(unix["cases"]) == 2,
        "current same-host Unix runtime proof",
    )
    for row in unix["cases"]:
        text = log(row["full-suite"])
        require(
            "test " + row["test"] + " ... ok" in text,
            "native runtime test actually executed",
        )
    diagnostic = read("native-eri-next-registry-20261022-v1.json")
    require(
        diagnostic["state"] == "finished" and diagnostic["exit-code"] == 0,
        "read-only next registry diagnostic",
    )
    for step in diagnostic["steps"]:
        text = log(step)
        if step["name"] == "whoami":
            require(
                "terminal turn=2832" in text
                and "Display" in text
                and "length-bytes=122" in text
                and "flags=240" in text,
                "exact next registry request",
            )
    links = read("macos-eri-candidate1-linkage.json")
    require(
        len(links["targets"]) == 8
        and all(x["selected-carrier-in-link-rule"] for x in links["targets"]),
        "actual carrier links",
    )
    require(
        "100% tests passed out of 7"
        in (directory / "macos-eri-candidate1-owning-ctest.log").read_text(),
        "seven CTests",
    )
    return {
        "state": "verified",
        "suites": suite,
        "owning": owning,
        "macos-carrier-links": 8,
        "macos-ctests": 7,
        "native-runtime-DLL-union": 10,
        "ordinary-new-boundary": "NtOpenKey",
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
