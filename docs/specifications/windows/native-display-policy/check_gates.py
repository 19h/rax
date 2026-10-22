"""Check captured Display-policy suites, selected archives, and caller outcomes."""

import importlib.util
import json
from pathlib import Path
import re


# Embedded Python can omit the script directory from its isolated import path.
spec = importlib.util.spec_from_file_location(
    "display_native_evidence", Path(__file__).with_name("check_native_evidence.py")
)
evidence = importlib.util.module_from_spec(spec)
spec.loader.exec_module(evidence)
HERE = evidence.HERE
check_catalog = evidence.check_catalog
load = evidence.load
matches = evidence.matches
require = evidence.require


def steps(base, name):
    receipt = load(base / name)
    code = receipt.get("exit-code", receipt.get("exit_code"))
    require(receipt["state"] == "finished" and code == 0, "unfinished gate " + name)
    projections = {
        row["path"]: row for row in load(base / "public-projections.json")["files"]
    }
    result = {}
    for row in receipt["steps"]:
        key = row.get("name", row.get("gate"))
        require(key not in result, "duplicate gate " + str(key))
        log = base / row["log"]
        projection = projections.get(row["log"])
        if projection:
            require(
                all(
                    row[item] == projection["original"][item]
                    for item in ["bytes", "sha256"]
                ),
                "projection origin",
            )
            matches(log, projection["published"])
            if projection["kind"] == "cargo-artifact-only":
                lines = [json.loads(line) for line in log.read_text().splitlines()]
                require(
                    lines
                    and all(item["reason"] == "compiler-artifact" for item in lines),
                    "compiler projection",
                )
        else:
            matches(log, row)
        require(row["exit-code"] == 0 or key == "full", "failed gate " + str(key))
        result[key] = (row, log.read_text(errors="replace"))
    return receipt, result


def total(gates, name, expected):
    require(name in gates, "missing suite " + name)
    require(expected in gates[name][1], "suite result " + name)


def archives(base, before, after):
    first = load(base / before)
    last = load(base / after)
    require(first["archives"] == last["archives"], "owning archive changed")
    require(
        first["core-features"] == [] and last["core-features"] == [],
        "owning core features",
    )
    require(len(first["sources"]) == 3969, "owning source envelope")
    require(len(first["archives"]) == 2, "owning archive selection")
    for row in first["archives"]:
        require(
            row["bytes"] > 0
            and row["members"] > 0
            and row["structural-member-walk"] == "pass",
            "archive member walk",
        )


def main():
    check_catalog(HERE)
    base = HERE / "validation"
    for host, count in [("macos", 8176), ("linux", 8170)]:
        receipt, gates = steps(base, f"display-{host}-candidate1-gates.json")
        require(
            (
                receipt["source-inputs"] == 29015
                if "source-inputs" in receipt
                else receipt["post-source-inputs"] == 29015
            ),
            "standalone source envelope",
        )
        total(gates, "full", f"{count} passed; 0 failed; 2 ignored;")
        total(gates, "focus", "4 passed; 0 failed;")
        total(gates, "capi", "183 passed; 0 failed;")
        total(gates, "integration", "544 passed; 0 failed;")
        require("all-targets" in gates, "all-targets gate")
    receipt, gates = steps(base, "display-candidate1-windows-retry1.json")
    require(receipt["source-inputs"] == 29015, "Windows source envelope")
    total(gates, "focus", "7 passed; 0 failed;")
    total(gates, "cpu-adapter", "5 passed; 0 failed;")
    total(
        gates, "full", "7648 passed; 5 failed; 2 ignored; 0 measured; 0 filtered out;"
    )
    require(
        gates["full"][0]["exit-code"] == 101
        and len(receipt["exact-existing-failures"]) == 5,
        "Windows failures concealed",
    )
    actual = gates["full"][1].split("\nfailures:\n")[-1].split("\ntest result:")[0]
    names = {line.strip() for line in actual.splitlines() if line.startswith("    ")}
    require(names == set(receipt["exact-existing-failures"]), "Windows failure set")
    total(gates, "capi", "183 passed; 0 failed;")
    total(gates, "integration", "4 passed; 0 failed;")
    require("all-targets" in gates, "Windows all-targets gate")
    for host in ["macos", "linux"]:
        receipt, gates = steps(base, f"{host}-display-candidate1-owning-gates.json")
        require(receipt["archive-identities-stable"], "owning identities")
        archives(
            base,
            f"{host}-display-candidate1-archives-before.json",
            f"{host}-display-candidate1-archives-after.json",
        )
        if host == "macos":
            total(gates, "ctest", "100% tests passed out of 7")
            passed = re.findall(
                r"^\d+/7 Test #\d+: .*\bPassed\b", gates["ctest"][1], re.M
            )
            require(len(passed) == 7, "native CTest pass count")
        else:
            total(gates, "cpp", "linked-archive-after")
    steps(base, "display-native-owning-candidate1.json")
    archives(
        base,
        "native-display-candidate1-pre-cpp-archives.json",
        "native-display-candidate1-final-owning-archive-identities.json",
    )
    rust, rust_gates = steps(base, "display-native-ordinary-candidate1.json")
    require(
        "terminal-call=974 result=Complete(Exited(0))" in rust_gates["smoke"][1],
        "Rust smoke result",
    )
    require(
        "terminal-call=2866" in rust_gates["whoami"][1]
        and "NtQueryInformationProcess class 0" in rust_gates["whoami"][1],
        "Rust next frontier",
    )
    capi, _ = steps(base, "display-owning-capi-candidate1.json")
    require(len(capi["outcomes"]) == 2, "C ABI outcomes")
    smoke, program = capi["outcomes"]
    require(
        (smoke["reason"], smoke["exit-code"], smoke["turns-started"]) == (4, 0, 975),
        "C ABI smoke result",
    )
    require(
        program["reason"] == 5
        and program["turns-started"] == 2867
        and "NtQueryInformationProcess class 0" in program["diagnostic"],
        "C ABI next frontier",
    )
    compiler, checks = steps(base, "display-compiler-gates-v1.json")
    require(set(checks) == {"format", "clippy"}, "compiler checks")
    require(compiler["exit_code"] == 0, "compiler gate failure")
    steps(base, "native-display-next-process-20261022-v1.json")
    print(
        "verified macOS/Linux/Windows captured gates, archive stability, and bounded caller outcomes"
    )


if __name__ == "__main__":
    main()
