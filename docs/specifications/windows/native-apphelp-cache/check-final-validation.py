"""Check platform, embedding and ordinary-process evidence without hiding skips."""
from pathlib import Path
import hashlib
import json
import re
from importlib.util import module_from_spec, spec_from_file_location
import sys

sys.dont_write_bytecode = True
BASE = Path(__file__).resolve().parent
RAX = BASE.parents[3]


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def load(name):
    return json.loads((BASE / name).read_text(encoding="utf-8-sig"))


def result(name):
    text = (BASE / name).read_text()
    matches = re.findall(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; \d+ measured; (\d+) filtered out", text)
    require(bool(matches), "test result missing: " + name)
    return [tuple(map(int, row)) for row in matches]


def failures(name):
    return set(re.findall(r"^test (.+) \.\.\. FAILED$", (BASE / name).read_text(), re.MULTILINE))


def verify():
    require(__debug__, "optimized validation is rejected")
    spec = spec_from_file_location("apphelp_native_evidence", BASE / "verify-evidence.py")
    evidence = module_from_spec(spec)
    spec.loader.exec_module(evidence)
    native = evidence.verify(BASE)
    expected = {"macos": (7784, 0, 2, 0), "linux": (7778, 0, 2, 0), "windows": (7202, 5, 2, 0)}
    files = {"macos": "macos-full-final.log", "linux": "linux-full-full.log", "windows": "native-apphelp-live-full.log"}
    for host, file in files.items():
        require(result(file) == [expected[host]], "complete library scope: " + host)
    require(failures(files["windows"]) == failures("native-apphelp-full-full.log") and len(failures(files["windows"])) == 5, "unchanged Windows baseline failure set")
    for host, prefix in [("macos", "macos-full"), ("linux", "linux-full"), ("windows", "native-apphelp-full")]:
        require(result(prefix + "-capi.log")[0] == (168, 0, 0, 0), "C API suite: " + host)
        wanted = [(0, 0, 0, 0), (4, 0, 0, 0)] if host == "windows" else [(544, 0, 0, 0), (0, 0, 0, 0)]
        require(result(prefix + "-integration.log") == wanted, "integration selection/skips: " + host)
        text = (BASE / (prefix + "-all-targets.log")).read_text()
        require("Finished `dev`" in text and "error:" not in text, "all-target build: " + host)
    require(result("native-apphelp-live-focused.log") == [(148, 0, 0, 7061)], "final Windows focused gate")
    require("live installed CDB controls: 168 including one unknown name" in (BASE / "native-apphelp-live-focused.log").read_text(), "live kernel catalog comparator")
    require("100% tests passed out of 7" in (BASE / "macos-owning-ctest.log").read_text(), "seven owning CTests")
    for name in ["macos-owning-gates.json", "linux-owning-gates.json"]:
        require(all(row["exit-code"] == 0 for row in load(name)), "owning gates: " + name)
    for name in ["linux-owning-cpp.log", "assist-native-apphelp-owned-final-cpp.log"]:
        require("PASS" in (BASE / name).read_text() and "FAIL" not in (BASE / name).read_text(), "native owning C++ consumers: " + name)
    manifest = load("apphelp-owning-source-hashes.json")
    require(len(manifest["sources"]) == 219, "219 scoped source files")
    for item in manifest["sources"]:
        data = (RAX / item["path"]).read_bytes()
        require(len(data) == item["bytes"] and hashlib.sha256(data).hexdigest() == item["sha256"], "final source bytes: " + item["path"])
    for name in ["macos-owning-archive-identities.json", "linux-owning-archive-identities.json", "native-apphelp-final-owning-archive-hashes.json"]:
        row = load(name)
        require(row["core-features"] == [] and len(row["archives"]) == 2, "owning core feature/archive selection")
        require(all(a["structural-member-walk"] == "pass" and a["bytes"] > 0 and a["members"] > 0 for a in row["archives"]), "complete archive walk")
    cpp = load("apphelp-owning-root-scope.json")["consumers"]
    require(len(cpp) == 9 and all(row["same"] for row in cpp), "nine matching current owning C++ consumers")
    ordinary = (BASE / "native-apphelp-final-after-ordinary-processes.log").read_text()
    require(len(re.findall(r"^program ", ordinary, re.MULTILINE)) == 4, "four ordinary native samples")
    require(len(re.findall(r"^run reason=5 exit=0", ordinary, re.MULTILINE)) == 4, "ordinary samples still fail internally, not successful guest exits")
    require(ordinary.count("NT section outside selected installed image namespace") == 3 and ordinary.count("native NTDLL service NtTraceEvent") == 1, "ordinary continuation boundaries")
    trace = (BASE / "native-apphelp-extended-trace-output.log").read_text()
    require('terminal turn=2114258 Complete(Internal("native NTDLL service NtOpenSection' in trace, "unchanged extended observer continuation")
    require("diagnostic turn budget exhausted" not in trace, "extended observer reached a terminal result")
    source = (BASE / "native-apphelp-extended-trace.rs").read_bytes()
    require(len(source) == 11178 and hashlib.sha256(source).hexdigest() == "514fdbc99de6f562002aa8a198deb79e7f5cd012db058aa1460a0e5730ae37c0", "budget-only observer source")
    original = source.replace(b"0..3000000", b"0..1000000").replace(b"turn == 2999999", b"turn == 999999")
    require(hashlib.sha256(original).hexdigest() == "d41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e", "exact original observer after two budget replacements")
    name_trace = (BASE / "native-apphelp-section37-trace-output.log").read_text()
    require('text=Some("\\\\Windows\\\\SharedSection")' in name_trace and "terminal turn=2114258" in name_trace, "read-only next section name")
    stress = load("evidence-stress-results.json")
    require(len(stress) == 6 and all(row["rejected"] for row in stress), "six corrupt evidence rejections")
    return {"native": native, "full-libraries": expected, "owning-scoped-rust-sources": 219, "matching-current-cpp-consumers": 9, "ordinary-guest-exits": 0, "ordinary-next-section": "\\Windows\\SharedSection", "observer-next-turn": 2114258}


if __name__ == "__main__":
    print(json.dumps(verify(), indent=2))
