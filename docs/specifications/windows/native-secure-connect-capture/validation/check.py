"""Portable verification of collected raw evidence and compiled code snapshots."""
from pathlib import Path
import hashlib
import json
import re

ROOT = Path(__file__).parent


def require(condition, message):
    if not condition:
        raise ValueError(message)


def summaries(path):
    return [(int(p), int(f), int(i), int(filtered)) for p, f, i, filtered in
            re.findall(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;", path.read_text())]


def main():
    require(__debug__, "optimized verification rejected")
    results = json.loads((ROOT / "results.json").read_text())
    for row in results["files"]:
        data = (ROOT / row["path"]).read_bytes()
        require(len(data) == row["bytes"] and hashlib.sha256(data).hexdigest() == row["sha256"], row["path"])
    for row in json.loads((ROOT / "epoch2-frozen-code.json").read_text())["files"]:
        data = (ROOT / "compiled-source" / row["path"]).read_bytes()
        require(len(data) == row["bytes"] and hashlib.sha256(data).hexdigest() == row["sha256"], "compiled input " + row["path"])
    require(summaries(ROOT / "macos-v2-library-rerun.log") == [(7814, 0, 2, 0)], "complete unchanged macOS library rerun")
    require(summaries(ROOT / "linux/linux-lib.log") == [(7808, 0, 2, 0)], "complete Linux library")
    require(summaries(ROOT / "windows/native-secure-capture-full-full.log") == [(7235, 5, 2, 0)], "complete Windows known-failure library")
    for path in ["macos-v2-capi-rerun.log", "linux/linux-capi.log", "windows/native-secure-capture-full-capi.log"]:
        require((168, 0, 0, 0) in summaries(ROOT / path), "C API actual count " + path)
    require(summaries(ROOT / "macos/macos-lib.log") == [(7813, 1, 2, 0)], "retained initial macOS timer failure")
    require((167, 1, 0, 0) in summaries(ROOT / "macos/macos-capi.log"), "retained initial macOS native printf timeout")
    text = (ROOT / "windows/native-secure-capture-full-full.log").read_text()
    expected = set(results["library"]["windows"]["known-failure-names"])
    observed = {name.rsplit("::", 1)[-1] for name in re.findall(r"^    (\S+)$", text, re.M)}
    require(expected <= observed, "all five retained Windows failure names")
    print("Verified collected raw hashes,11 compiled input snapshots,three-host library/C API counts and retained failures")


if __name__ == "__main__":
    main()
