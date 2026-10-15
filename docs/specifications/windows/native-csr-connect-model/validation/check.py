"""Portable verification of retained CSR build/runtime evidence."""
from pathlib import Path
import hashlib
import json
import re

ROOT = Path(__file__).parent


def require(ok, message):
    if not ok:
        raise ValueError(message)


def read(name):
    return (ROOT / name).read_text(encoding="utf-8-sig")


def summaries(name):
    return [tuple(map(int, row)) for row in re.findall(
        r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;",
        read(name))]


def main():
    require(__debug__, "optimized verification rejected")
    results = json.loads(read("results.json"))
    for row in results["files"]:
        data = (ROOT / row["path"]).read_bytes()
        require((len(data), hashlib.sha256(data).hexdigest()) ==
                (row["bytes"], row["sha256"]), "retained identity " + row["path"])
    frozen = json.loads(read("integrated-source-state.json"))["verified-new-code-inputs"]
    require(len(frozen) == 20, "twenty compiled execution inputs")
    source = ROOT.parents[4]
    for row in frozen:
        for path in [ROOT / "compiled-source" / row["path"], source / row["path"]]:
            data = path.read_bytes()
            require((len(data), hashlib.sha256(data).hexdigest()) ==
                    (row["bytes"], row["sha256"]), "compiled and published input " + row["path"])
    for host, count in [("macos", 7825), ("linux", 7819)]:
        require(summaries(f"{host}/{host}-lib.log") == [(count, 0, 2, 0)], host + " full library")
        require((168, 0, 0, 0) in summaries(f"{host}/{host}-capi.log"), host + " C API")
        require((544, 0, 0, 0) in summaries(f"{host}/{host}-windows.log"), host + " Windows integration")
        require(any(x[:3] == (10, 0, 0) for x in summaries(f"{host}/{host}-focused.log")), host + " CSR focus")
        require(any(x[:3] == (7, 0, 0) for x in summaries(f"{host}/{host}-lpc.log")), host + " LPC focus")
        gates = json.loads(read(f"{host}/gates.json"))
        require(len(gates) == 7 and all(x["exit-code"] == 0 for x in gates), host + " seven gates")
    require(summaries("windows/native-csr-model-full.log") == [(7247, 5, 2, 0)], "Windows complete known-failure library")
    for label, count in [("focused", 12), ("lpc", 7), ("capi", 168)]:
        require(any(x[:3] == (count, 0, 0) for x in summaries(f"windows/native-csr-model-{label}.log")), "Windows " + label)
    require((4, 0, 0, 0) in summaries("windows/native-csr-model-integration.log"), "four native Windows memory tests")
    require("Validated installed ARM64 NtCreateSection/NtSecureConnectPort/NtClose with matched native CSR metadata" in
            read("windows/native-csr-model-focused.log"), "actual installed ARM64 positive execution")
    text = read("windows/native-csr-model-full.log")
    tail = text[text.rfind("\nfailures:\n"):]
    observed = set(re.findall(r"^    (\S+)$", tail, re.M))
    require(observed == set(results["known-windows-failures"]), "exact unchanged Windows failure set")
    gates = json.loads(read("windows/native-csr-model-gates.json"))
    require(len(gates) == 6 and all(x["exit-code"] == (101 if x["gate"] == "full" else 0) for x in gates),
            "six completed Windows gates")
    manifest = json.loads(read("csr-owning-source-hashes.json"))
    require(len(manifest["sources"]) == 285, "scoped source count")
    for name in ["macos-owning-archive-identities.json", "linux-owning-archive-identities.json",
                 "windows/native-csr-model-owning-archive-hashes.json"]:
        row = json.loads(read(name))
        require(row["sources"] == manifest["sources"] and row["core-features"] == [], "actual archive source binding " + name)
        require(len(row["archives"]) == 2 and all(a["structural-member-walk"] == "pass" and a["members"] > 200 and
                a["bytes"] > 100000000 and len(a["sha256"]) == 64 for a in row["archives"]), "two complete archive records " + name)
    ctest = read("macos-owning-ctest.log")
    require("100% tests passed out of 7" in ctest and len(re.findall(r"^\d/7 Test .*Passed", ctest, re.M)) == 7,
            "seven macOS owning CTests")
    for host in ["linux", "windows"]:
        name = "linux-owning-cpp.log" if host == "linux" else "windows/assist-native-csr-model-cpp.log"
        text = read(name)
        for marker in results["cpp-consumer-markers"]:
            require(marker in text, host + " owning consumer " + marker)
    for host in ["macos", "linux"]:
        require(all(x["exit-code"] == 0 for x in json.loads(read(host + "-owning-gates.json"))), host + " owning gates")
    require(json.loads(read("windows/csr-model-owning-epoch3-atomic-v2-launch.json"))["exit-code"] == 0,
            "Windows owning driver completed")
    require("NtInitializeNlsFiles" in read("windows/native-csr-model-ordinary-0.log"), "new ordinary frontier retained")
    require(results["ordinary-windows-guest-exits"] == 0, "ordinary completion remains unproven")
    print("Verified 20 compiled inputs, three-host suites, exact five known failures, native CSR success, 285 source identities and scoped owning consumers")


if __name__ == "__main__":
    main()
