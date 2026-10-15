"""Check completed epoch2 gates without accepting filtered/partial evidence."""
from pathlib import Path
import hashlib
import json
import re

ROOT = Path(__file__).parent
SHARE = Path("/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154")


def require(value, message):
    if not value:
        raise ValueError(message)


def summaries(path):
    text = path.read_text(errors="strict")
    rows = re.findall(r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;", text)
    return text, [(kind, int(passed), int(failed), int(ignored), int(filtered))
                  for kind, passed, failed, ignored, filtered in rows]


def unix(host, expected_library):
    directory = ROOT / (host + "-v2")
    gates = json.loads((directory / (host + "-gates.json")).read_text())
    expected = {"focused", "lib", "capi", "all-targets", "windows", "build-user"}
    require(len(gates) == 6 and {row["gate"] for row in gates} == expected, host + " complete command partition")
    for row in gates:
        retained_timing_failure = host == "macos" and row["gate"] in {"lib", "capi"}
        require(row["exit-code"] == (101 if retained_timing_failure else 0), host + " " + row["gate"])
        require("--no-default-features" in row["command"], host + " portable feature selection")
        data = (directory / row["log"]).read_bytes()
        require(hashlib.sha256(data).hexdigest() == row["sha256"], host + " raw output identity")
    _, rows = summaries(directory / (host + "-focused.log"))
    require(len(rows) == 1 and rows[0][:4] == ("ok", 6, 0, 0), host + " all six focused cases")
    original, rows = summaries(directory / (host + "-lib.log"))
    if host == "macos":
        require(rows == [("FAILED", expected_library - 1, 1, 2, 0)], "retained initial macOS library failure")
        require("---- user::linux::tests::uring::timeout::a_multishot_timeout_reports_each_expiry stdout ----" in original,
                "exact retained multishot timer failure")
        retry = json.loads((ROOT / "macos-v2-library-rerun.json").read_text())
        require(retry["exit-code"] == 0, "complete unchanged macOS library rerun")
        require(hashlib.sha256((ROOT / retry["log"]).read_bytes()).hexdigest() == retry["sha256"], "library rerun raw identity")
        _, rows = summaries(ROOT / retry["log"])
        diagnostic = json.loads((ROOT / "macos-v2-timer-diagnostics.json").read_text())
        require(retry["binary"] == diagnostic["binary"], "exact previously observed test executable")
        require(len(diagnostic["unmodified-exact-test"]) == 3 and all(row["exit-code"] == 0 for row in diagnostic["unmodified-exact-test"]),
                "three unchanged timer diagnostic reruns")
    require(rows == [("ok", expected_library, 0, 2, 0)], host + " complete library selection")
    original, rows = summaries(directory / (host + "-capi.log"))
    if host == "macos":
        require(any(row == ("FAILED", 167, 1, 0, 0) for row in rows), "retained initial macOS C API failure")
        require("---- tests::process::native_runtime::installed_dynamic_programs_run_without_supplied_dependencies_or_general_host_access stdout ----" in original,
                "exact retained installed-runtime timeout")
        retry = json.loads((ROOT / "macos-v2-capi-rerun.json").read_text())
        require(retry["exit-code"] == 0, "complete unchanged macOS C API rerun")
        require(hashlib.sha256((ROOT / retry["log"]).read_bytes()).hexdigest() == retry["sha256"], "C API rerun raw identity")
        _, rows = summaries(ROOT / retry["log"])
    require(any(row == ("ok", 168, 0, 0, 0) for row in rows), host + " C API actual count")
    _, rows = summaries(directory / (host + "-windows.log"))
    require(any(row == ("ok", 544, 0, 0, 0) for row in rows), host + " Windows integration actual count")
    return gates


def windows():
    gates = json.loads((SHARE / "native-secure-capture-full-gates.json").read_text(encoding="utf-8-sig"))
    require(len(gates) == 5 and {row["gate"] for row in gates} ==
            {"focused", "full", "capi", "all-targets", "integration"}, "Windows complete command partition")
    for row in gates:
        require(row["exit-code"] == (101 if row["gate"] == "full" else 0), "Windows gate " + row["gate"])
        require("--no-default-features" in row["command"], "Windows portable feature selection")
    _, rows = summaries(SHARE / "native-secure-capture-full-focused.log")
    require(len(rows) == 1 and rows[0][:4] == ("ok", 7, 0, 0), "Windows seven actual focused tests, installed leaves included")
    text, rows = summaries(SHARE / "native-secure-capture-full-full.log")
    require(rows == [("FAILED", 7235, 5, 2, 0)], "Windows complete known-failure library count")
    expected = set(json.loads((ROOT / "final-validation-requirements.json").read_text())["known-windows-library-failures"])
    observed = {name.rsplit("::", 1)[-1] for name in re.findall(r"^    (\S+)$", text, re.M)
                if name.rsplit("::", 1)[-1] in expected}
    require(observed == expected, "Windows exact unchanged failure names")
    _, rows = summaries(SHARE / "native-secure-capture-full-capi.log")
    require(any(row == ("ok", 168, 0, 0, 0) for row in rows), "Windows C API actual count")
    _, rows = summaries(SHARE / "native-secure-capture-full-integration.log")
    require(any(row == ("ok", 4, 0, 0, 0) for row in rows), "Windows native memory integration actual count")
    return gates


def owning_unix(host):
    gates = json.loads((ROOT / (host + "-owning-gates.json")).read_text())
    required = {"build", "ctest", "protected-assist-cli", "protected-assist-mcp"} if host == "macos" else {"shipping", "cpp"}
    require(len(gates) == len(required) and {row["gate"] for row in gates} == required, host + " owning complete partition")
    for row in gates:
        require(row["exit-code"] == 0, host + " owning " + row["gate"])
        require(hashlib.sha256((ROOT / row["log"]).read_bytes()).hexdigest() == row["sha256"], host + " owning log identity")
    if host == "macos":
        text = (ROOT / "macos-owning-ctest.log").read_text()
        require("100% tests passed out of 7" in text and len(re.findall(r"\bPassed\b", text)) == 7, "seven actual macOS CTests")
    else:
        text = (ROOT / "linux-owning-cpp.log").read_text()
        require(len(re.findall(r"^run /validation/(\S+) /validation/\1$", text, re.M)) == 5,
                "five actual checked Linux C++ executions")
    return gates


def owning_windows():
    launch = json.loads((SHARE / "secure-capture-owning-atomic-v2-launch.json").read_text())
    require(launch["exit-code"] == 0, "Windows guarded owning producer terminal0")
    driver = (SHARE / "secure-capture-owning-atomic-v2-driver.log").read_text(encoding="utf-8-sig")
    for name in ("assist-native-secure-capture-shipping.log", "assist-native-secure-capture-cpp.log"):
        data = (SHARE / name).read_bytes()
        require(name + " " + hashlib.sha256(data).hexdigest().upper() in driver, "Windows actual/raw output identity " + name)
    text = (SHARE / "assist-native-secure-capture-cpp.log").read_text()
    runs = re.findall(r"^run (C:\\Windows\\Temp\\assist-native-rax-20261010\\assist\\native-cpp\\(\w+)\.exe) \1$", text, re.M)
    require(len(runs) == 5 and {name for _, name in runs} ==
            {"process_tool", "process_tool_unavailable", "rax_process", "rax_abi_drift", "rust_archive_link"}, "five actual Windows C++ executions")
    archives = json.loads((SHARE / "native-secure-capture-owning-archive-hashes.json").read_text())
    manifest = json.loads((ROOT / "secure-capture-source-hashes.json").read_text())
    require(archives["core-features"] == [], "Windows empty-feature owning core")
    require(len(archives["sources"]) == 274 and archives["sources"] == manifest["sources"], "Windows exact274 actual source identities")
    require(archives["helpers"] == manifest["helpers"] and archives["observer-sha256"] == manifest["observer-sha256"], "Windows preserved owning helper identities")
    require(len(archives["archives"]) == 2 and all(row["structural-member-walk"] == "pass" and row["members"] > 0 and row["bytes"] > 0
            and re.fullmatch("[a-f0-9]{64}", row["sha256"]) for row in archives["archives"]), "Windows complete static/rlib archive walks")
    source = json.loads((SHARE / "native-secure-capture-focused-source-verification.json").read_text())
    require(source["verified-count"] == len(source["files"]) == 316, "Windows exact scoped source/input verification")
    consumers = json.loads((SHARE / "native-secure-capture-current-cpp-verification.json").read_text())
    require(consumers["verified-count"] == 9 and consumers["files"] == json.loads((ROOT / "current-cpp-consumers-reverified.json").read_text())["files"],
            "Windows nine current/frozen C++ identities")
    return archives


def main():
    require(__debug__, "optimized validation rejected")
    unix("macos", 7814)
    unix("linux", 7808)
    windows()
    owning_unix("macos")
    owning_unix("linux")
    owning_windows()
    print("Completed epoch2 three-host core and scoped owning gates verified; retained macOS initial timing failures and Windows5 known failures remain explicit")


if __name__ == "__main__":
    main()
