from pathlib import Path
import hashlib
import json
import re

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


require(__debug__, "optimized validation rejected")
summary = json.loads((HERE / "summary.json").read_text())
for row in summary["artifacts"]:
    path = HERE / row["path"]
    require(path.stat().st_size == row["bytes"] and digest(path) == row["sha256"], row["path"])
for row in json.loads((HERE / "feature-files-before-rebase.json").read_text())["files"]:
    path = ROOT / row["path"]
    require(path.stat().st_size == row["bytes"] and digest(path) == row["sha256"], "feature " + row["path"])
for row in json.loads((HERE / "shared-section-integration-sources.json").read_text())["files"]:
    path = ROOT / row["path"]
    require(path.stat().st_size == row["bytes"] and digest(path) == row["sha256"], "source " + row["path"])
for host, counts in summary["library-counts"].items():
    name = host + "-lib.log" if host != "windows" else "native-shared-section-integration-full-full.log"
    log = (HERE / name).read_text()
    match = re.search(r"test result: .*? (\d+) passed; (\d+) failed; (\d+) ignored;.*?0 filtered out", log)
    require(match and list(map(int, match.groups())) == counts, host + " library")
    if host == "windows":
        tail = log[log.rfind("failures:"):]
        for failure in summary["known-windows-failures"]:
            require(tail.count(failure) == 1, failure)
    name = host + "-capi.log" if host != "windows" else "native-shared-section-integration-full-capi.log"
    require("168 passed; 0 failed; 0 ignored;" in (HERE / name).read_text(), host + " capi")
for row in json.loads((HERE / "commands.json").read_text()):
    require(row["exit-code"] == (101 if row["host"] == "windows" and row["gate"] == "full" else 0), "gate " + row["host"] + row["gate"])
for host in ["macos", "linux"]:
    require("42 passed; 0 failed; 1 ignored;" in (HERE / (host + "-linux.log")).read_text(), host + " serial integration")
for name in ["macos-owning-archives.json", "linux-owning-archives.json", "native-shared-section-integration-owning-archive-hashes.json"]:
    record = json.loads((HERE / name).read_text())
    require(record["core-features"] == [] and len(record["sources"]) == 249, name)
    for row in record["sources"]:
        require(digest(ROOT / row["path"]) == row["sha256"], "owning source " + row["path"])
    require(all(row["structural-member-walk"] == "pass" and row["members"] > 0 for row in record["archives"]), "archive walk")
require(summary["ordinary-windows-guest-exits"] == 0, "ordinary guest result")
require("100% tests passed out of 7" in (HERE / "macos-owning-ctest.log").read_text(), "scoped ctest")
print("Verified unchanged106 feature blobs,8475 inputs,3host library/CAPI gates,249-unit embeddings and explicit baseline failures")
