from pathlib import Path
import hashlib
import json
import re

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
V = HERE / "validation"


def require(ok, message):
    if not ok:
        raise ValueError(message)


def identity(path, row):
    data = path.read_bytes()
    require(len(data) == row["bytes"] and hashlib.sha256(data).hexdigest() == row["sha256"], str(path))


require(__debug__, "optimized validation rejected")
summary = json.loads((V / "summary.json").read_text())
for row in summary["artifacts"]:
    identity(V / row["path"], row)
for row in json.loads((V / "reclaim-pressure-sources.json").read_text())["files"]:
    identity(ROOT / row["path"], row)
require(len(json.loads((V / "reclaim-pressure-sources.json").read_text())["files"]) == 8537, "selected input count")
for row in json.loads((V / "owned-source-hashes.json").read_text())["files"]:
    identity(ROOT / row["path"], row)
for host, expected in summary["library-counts"].items():
    prefix = host if host != "windows" else "native-reclaim-pressure-full"
    lib = (V / (prefix + ("-lib.log" if host != "windows" else "-full.log"))).read_text()
    result = re.search(r"test result: .*? (\d+) passed; (\d+) failed; (\d+) ignored;.*?0 filtered out", lib)
    require(result and list(map(int, result.groups())) == expected, host + " full-library counts")
    if host == "windows":
        tail = lib[lib.rfind("failures:"):]
        require(all(tail.count(name) == 1 for name in summary["known-windows-failures"]), "unchanged Windows failure set")
    require("6 passed; 0 failed; 0 ignored;" in (V / (prefix + "-focused.log")).read_text(), host + " six pressure tests")
    require("168 passed; 0 failed; 0 ignored;" in (V / (prefix + "-capi.log")).read_text(), host + " C API")
for host in ["macos", "linux"]:
    require("42 passed; 0 failed; 1 ignored;" in (V / (host + "-linux.log")).read_text(), host + " Linux integration")
    require("544 passed; 0 failed; 0 ignored;" in (V / (host + "-windows.log")).read_text(), host + " Windows personality")
require("117 passed; 0 failed; 0 ignored;" in (V / "macos-darwin.log").read_text(), "actual pinned native Darwin suite")
require("4 passed; 0 failed; 0 ignored;" in (V / "native-reclaim-pressure-full-integration.log").read_text(), "actual Windows native memory tests")
commands = json.loads((V / "commands.json").read_text())
require(len(commands) == 27, "eight gates/Unix host,five Windows and six scoped owning gates")
for row in commands:
    require(row["exit-code"] == (101 if row["host"] == "windows" and row["gate"] == "full" else 0), "actual gate " + row["host"] + row["gate"])
for name in ["macos-owning-archives.json", "linux-owning-archives.json", "native-reclaim-pressure-owning-archive-hashes.json"]:
    row = json.loads((V / name).read_text())
    require(row["core-features"] == [] and len(row["sources"]) == 273, name + " scoped source/features")
    for source in row["sources"]:
        identity(ROOT / source["path"], source)
    require(all(a["structural-member-walk"] == "pass" for a in row["archives"]), name + " whole archive walks")
ctest = (V / "macos-owning-ctest.log").read_text()
require("100% tests passed out of 7" in ctest and ctest.count("   Passed") == 7, "seven owning CTests")
for name in ["assist-cli", "assist-mcp"]:
    require("0 failures" in (V / ("macos-owning-protected-" + name + ".log")).read_text(), "protected text " + name)
for name in ["assist-native-reclaim-pressure-cpp.log", "linux-owning-cpp.log"]:
    executed = []
    for line in (V / name).read_text().splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[0] == "run" and parts[1] == parts[2]:
            executed.append(parts[1].replace(chr(92), "/").rsplit("/", 1)[-1].removesuffix(".exe"))
    require(len(executed) == 5 and set(executed) == {"process_tool", "process_tool_unavailable", "rax_process", "rax_abi_drift", "rust_archive_link"}, "five checked child executions " + name)
    require((V / name).read_text().count("PASS") == 1, "archive contract marker " + name)
comparison = json.loads((V / "workloop-comparison-v2-results.json").read_text())
require(len(comparison) == 4 and all(r["exit-code"] == 0 for r in comparison), "valid comparison control executions")
reference = comparison[0]["stdout-sha256"]
require(comparison[1]["stdout-sha256"] == comparison[3]["stdout-sha256"] == reference and comparison[2]["stdout-sha256"] != reference, "retained nightly-only difference; exit0 is not proof of matching behavior")
require("dispatch priority: waited=49 ran=0" in (V / "workloop-comparison-v2-current-nightly.log").read_text(), "explicit supplementary workloop timeout")
require("116 passed; 1 failed;" in (V / "supplementary-nightly-macos-darwin.log").read_text(), "supplementary failure retained")
pinned = json.loads((V / "emulated-pinned-after-outcome.json").read_text())
require(pinned["exit-code"] == 0 and pinned["toolchain"].startswith("rustc 1.95.0 "), "actual pinned runtime control")
print("Verified8537 inputs,3host Rust1.95 library/CAPI/native gates,273 scoped embedding inputs,full archive walks and retained supplementary failures")
