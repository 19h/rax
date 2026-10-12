"""Check actual producer identities, independent partitions and retained gates.

The native kernel is the semantic oracle; Rust replay compares every captured
row with the emulator. This checker derives input partitions from the retained
C++ producer loops and separately checks resource and output invariants.
"""
from pathlib import Path
import hashlib
import itertools
import json
import re
import sys


def require(ok, message):
    if not ok:
        raise ValueError(message)


require(not sys.flags.optimize, "optimized validation rejected")
base = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent
source = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(__file__).resolve().parents[4]
native_only = "--native-only" in sys.argv


def read(name):
    return (base / name).read_text(encoding="utf-8-sig")


def data(name):
    return json.loads(read(name))


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def rows(name, prefix="case="):
    return [dict(f.split("=", 1) for f in line.split() if "=" in f)
            for line in read(name).splitlines() if line.startswith(prefix)]


def num(row, key, radix=10):
    return int(row[key], radix)


def partition(observed, keys, expected, label):
    require([num(r, "case") for r in observed] == list(range(len(expected))),
            "sequential cases " + label)
    require([tuple(num(r, k, radix) for k, radix in keys) for r in observed] == expected,
            "independent input partition " + label)


profiles = ["arm64", "x64", "x86", "x86-laa"]
families = {"matrix": 967, "boundaries": 5000, "format": 762,
            "reference": 2064, "duplicate-boundaries": 112,
            "privilege": 72, "flag-boundaries": 420,
            "duplicate-policy": 1200, "name-probe": 6656,
            "duplicate-protected-boundaries": 168, "snapshot": 0}
prefix = "native-object-directory-open-"
identity = data(prefix + "producer-identities.json")
require(identity["windows-version"] == "10.0.29683" and
        identity["host-machine"] == "ARM64", "measured host identity")
expected_producers = {(f, p) for f in families for p in profiles} | {("fresh", "arm64")}
require(len(identity["producers"]) == 45 and
        {(r["family"], r["profile"]) for r in identity["producers"]} == expected_producers,
        "exact actual producer set")
for producer in identity["producers"]:
    for key, name in [("source_identity", producer["source"]),
                      ("log_identity", producer["log"])]:
        actual = producer[key]
        require(actual["bytes"] == (base / name).stat().st_size and
                actual["sha256"] == sha(base / name), "actual producer " + key)
    require(producer["source-sha256"] == producer["source_identity"]["sha256"],
            "producer source binding")
    executable = producer["executable_identity"]
    require(executable["machine"] == {"arm64": 0xAA64, "x64": 0x8664,
            "x86": 0x14C, "x86-laa": 0x14C}[producer["profile"]], "actual PE architecture")
    require(executable["bytes"] > 0 and re.fullmatch("[a-f0-9]{64}", executable["sha256"]),
            "actual PE identity")
    if producer["profile"] == "x86-laa":
        require(executable["characteristics"] & 0x20, "actual PE LAA flag")
require([r["sha256"] for r in identity["native-libraries"]] == [
    "502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f",
    "ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e"],
    "installed NTDLL identities")
require("nt authority\\system" in read(prefix + "principal.csv").lower() and
        "S-1-5-18" in read(prefix + "principal.csv"), "actual capture principal")
accesses = [0, 1, 2, 3, 4, 8, 15, 0x10000, 0x20000, 0x40000, 0x80000,
            0xF0000, 0x100000, 0x01000000, 0x02000000, 0x80000000,
            0x40000000, 0x20000000, 0x10000000, 0xFFFFFFFF]
names = ["\\KnownDlls", "\\KnownDlls32", "\\knowndlls", "KnownDlls", "\\", "",
         "\\KnownDlls\\", "\\KnownDlls\\\\x", "\\KnownDlls\\rax-absent-20261012",
         "\\rax-absent-20261012\\x", "\\KnownDlls\\KnownDllPath",
         "\\KnownDlls\\ntdll.dll", "KnownDllPath", "ntdll.dll"]
format_names = ["\\KnownDlls", "\\KnownDlls\\KnownDllPath", "\\KnownDlls\\ntdll.dll",
                "\\KnownDlls\\ntdll.dll\\x", "\\KnownDlls\\KnownDllPath\\x", "\\KnownDlls\\",
                "\\KnownDlls\\\\x", "\\\\KnownDlls", "\\.\\KnownDlls", "\\KnownDlls/ntdll.dll",
                "\\KnownDlls\\..", "\\KnownDlls\\.", "\\", "\\rax-absent-20261012\\x", "\\knowndlls"]
keysets = {
    "matrix": [(k, 16 if k in {"access", "flags"} else 10) for k in
               ["family", "name", "access", "flags", "attributes-length", "root", "mode", "name-length", "maximum"]],
    "boundaries": [(k, 10) for k in ["attributes", "output", "unicode", "buffer", "attributes-length"]],
    "flag-boundaries": [(k, 16 if k == "flags" else 10) for k in
                        ["attributes", "output", "unicode", "buffer", "attributes-length", "name-length", "flags"]],
    "format": [(k, 16 if k in {"flags", "access"} else 10) for k in
               ["family", "path", "length", "flags", "unicode-offset", "buffer-offset", "security", "qos", "access"]],
    "reference": [(k, 16 if k in {"source-access", "desired", "options"} else 10) for k in
                  ["family", "parent", "source-access", "tag", "name", "desired", "options"]],
    "duplicate-policy": [(k, 16 if k not in {"family", "parent"} else 10) for k in
                         ["family", "parent", "source-access", "desired", "options", "source-flags"]],
    "privilege": [("phase", 10), ("path", 10), ("access", 16)],
    "name-probe": [(k, 16 if k == "flags" else 10) for k in
                   ["flags", "root", "output", "pattern", "length", "layout", "prefix"]],
    "duplicate-boundaries": [("desired", 16), ("options", 16), ("output-kind", 10)],
    "duplicate-protected-boundaries": [("desired", 16), ("options", 16), ("output-kind", 10)],
}
all_rows = {}
total = 0
for profile in profiles:
    width = 4 if profile.startswith("x86") else 8
    attrs = 24 if width == 4 else 48
    matrix = [(0, n, a, f, attrs, 0, 0, len(names[n]) * 2, len(names[n]) * 2 + 2)
              for n, a, f in itertools.product(range(14), accesses, [0, 0x40])]
    matrix += [(1, n, 3, f, attrs, 0, 0, len(names[n]) * 2, len(names[n]) * 2 + 2)
               for f, n in itertools.product([0, 1, 2, 4, 8, 0x10, 0x20, 0x40, 0x80, 0x100,
                                              0x200, 0x400, 0x800, 0x1000, 0x2000, 0xFFFFFFFF], [0, 2, 8])]
    matrix += [(2, 0, 3, 0x40, l, 0, m, 20, 22) for l, m in
               itertools.product([0, 1, 4, 23, 24, 25, 47, 48, 49, 0xFFFFFFFF], range(7))]
    matrix += [(3, n, 3, 0x40, attrs, r, 0, len(names[n]) * 2, len(names[n]) * 2 + 2)
               for r, n in itertools.product(range(7), [0, 3, 4, 5, 8, 12, 13])]
    matrix += [(4, 0, 3, 0x40, attrs, 0, m, l, (l + 2) & 65535 if mx == 0xFFFFFFFF else mx)
               for l, mx, m in itertools.product([0, 1, 2, 19, 20, 21, 22, 24, 65534, 65535],
                                                [0, 1, 19, 20, 21, 22, 65535, 0xFFFFFFFF], [0, 2, 3])]
    formats = [(0, p, len(format_names[p]) * 2, f, 0, 0, 0, 0, a) for p, f, a in
               itertools.product(range(15), [0, 0x40, 0x400, 0x440, 0x1000, 0x1040],
                                 [3, 0x01000000, 0x03000000, 0xFFFFFFFF])]
    formats += [(1, 0, l, f, nk, bk, 0, 0, 3) for l, nk, bk, f in
                itertools.product([0, 1, 2, 3, 19, 20, 21, 22, 65530, 65532, 65534, 65535],
                                  range(4), range(4), [0x40, 1])]
    formats += [(2, 0, 20, f, 0, 0, sk, qk, 3) for sk, qk, f in
                itertools.product(range(3), range(3), [0x40, 1])]
    reference = [(0, p, a, t, n, 0, 0) for p, a, t, n in
                 itertools.product(range(3), [1, 2, 3, 0x20000, 0x20003, 0x02000000], range(6), range(8))]
    reference += [(1, p, a, 0, 0, d, o) for p, a, d, o in
                  itertools.product(range(3), [1, 2, 0x20000, 0x02000000], accesses, range(5))]
    flags = [(0, o, 0, b, attrs, l, f) for f, o, b, l in
             itertools.product([0x40, 1, 0x20, 0x60, 0x100, 0xFFFFFFFF], [0, 2, 3], range(5), [0, 1, 20, 65534])]
    flags += [(0, o, n, 0, attrs, 20, f) for f, o, n in
              itertools.product([0x40, 1, 0x20, 0x60, 0x100, 0xFFFFFFFF], [0, 3], range(5))]
    prefixes = [0, 2, 4, 6, 8, 10, 14, 16, 18, 20, 24, 26, 32]
    expected = {
        "matrix": matrix, "format": formats, "reference": reference, "flag-boundaries": flags,
        "boundaries": list(itertools.product(range(10), range(10), range(5), range(5), [0, attrs])),
        "duplicate-boundaries": list(itertools.product([1, 4, 0x01000000, 0xFFFFFFFF], range(4), range(7))),
        "duplicate-protected-boundaries": list(itertools.product([1, 4, 0x01000000, 0xFFFFFFFF], [0, 1, 4, 5, 6, 7], range(7))),
        "duplicate-policy": [(1, p, a, d, o, f) for p, a, d, o, f in
                             itertools.product(range(3), [1, 0x02000000], [1, 2, 4, 0x01000000, 0xFFFFFFFF],
                                               [0, 1, 2, 3, 4, 5, 6, 7, 8, 0xFFFFFFFF], range(4))],
        "privilege": list(itertools.product(range(3), range(3), [0x01000000, 0x01000001, 0x01000004,
                           0x03000000, 0x03000003, 0x010F000F, 0x02000000, 0xFFFFFFFF])),
        "name-probe": [(f, r, o, p, l, k, prefixes[k]) for f, r, o, p, l, k in
                       itertools.product([0, 0x40], range(2), range(4), range(4),
                                         [0, 1, 2, 18, 20, 22, 64, 65532], range(13))],
    }
    for family, inputs in expected.items():
        name = prefix + family + "-" + profile + ".log"
        observed = rows(name)
        # Reference includes two differently shaped record families.
        if family == "reference":
            observed = [dict(r, **{"desired": "0", "options": "0"}) if r["family"] == "0"
                        else dict(r, tag="0", name="0") for r in observed]
        partition(observed, keysets[family], inputs, family + "/" + profile)
        require(len(observed) == families[family] and f"complete cases={len(inputs)}" in read(name),
                "complete producer " + family)
        for row in observed:
            require(num(row, "width") == width, "actual caller width")
            require(row["handles-before"] == row["handles-after"], "owned handle restoration")
            if "exception" in row:
                require(num(row, "exception", 16) == 0, "native wrapper exception containment")
            if "status" in row and num(row, "status", 16) == 0 and "close" in row:
                require(num(row, "close", 16) == 0, "successful directory close")
            if "dos" in row:
                mapping = {0: 0, 0xC0000005: 998, 0x80000001: 0x80000001,
                           0x80000002: 998, 0xC000000D: 87, 0xC0000022: 5,
                           0xC0000033: 123, 0xC0000034: 2, 0xC000003B: 161,
                           0xC000003A: 3, 0xC0000024: 6, 0xC0000008: 6,
                           0xC0000058: 1305, 0xC0000061: 1314, 0xC000050B: 4395}
                status = num(row, "status", 16)
                require(status in mapping and num(row, "dos") == mapping[status],
                        "independent RTL status mapping")
        all_rows[(family, profile)] = observed
        total += len(observed)
    require("token-restored=1 primary-unchanged=1 thread-restored=1" in
            read(prefix + "privilege-" + profile + ".log"), "isolated token and subject restoration")
    snap = prefix + "snapshot-" + profile + ".log"
    entries = rows(snap, "entry ")
    require(len(entries) == 151 and "complete directories=3 entries=151 links=2" in read(snap),
            "complete namespace snapshot")
    for directory in range(3):
        selected = [r for r in entries if num(r, "directory") == directory]
        require([num(r, "ordinal") for r in selected] == list(range(len(selected))),
                "namespace ordinal partition")
        require(len({r["name"] for r in selected}) == len(selected), "unique namespace entries")
    complete = rows(snap, "complete ")[0]
    require(complete["handles-before"] == complete["handles-after"], "snapshot native handle drain")
require(total == 69684, "exact mature syscall/duplicate total")
for family in keysets:
    # Compare kernel status/guard/output policy on matching caller-width profiles,
    # omitting unstable native addresses, counts and allocation-specific handles.
    for left, right in [("arm64", "x64"), ("x86", "x86-laa")]:
        for a, b in zip(all_rows[(family, left)], all_rows[(family, right)]):
            for key in ["status", "exception", "ok", "error", "grant", "flags", "source-live",
                        "buffer-protect", "output-protect", "attributes-protect", "unicode-protect"]:
                if key in a:
                    require(a[key] == b[key], "same-width native semantic agreement " + family + "/" + key)
fresh = read(prefix + "fresh-arm64.log")
require("complete cases=2 warmup=1" in fresh, "two complete fresh captures")
for row in rows(prefix + "fresh-arm64.log"):
    if row["warmup"] == "1":
        require(row["case"] == "0", "fresh warmup excluded from complete cases")
        continue
    require(row["installed"] == "1" and row["done"] == "1" and row["calls"] == "1" and
            row["exit"] == "77" and row["exact-handle-set"] == "1" and
            row["handles-before"] == row["handles-after"], "fresh owned-child termination and drain")
if not native_only:
    manifest = data("evidence-manifest.json")
    actual = {str(p.relative_to(base)) for p in base.rglob("*")
              if p.is_file() and p.name != "evidence-manifest.json"}
    require(set(manifest) == actual, "exact evidence coverage")
    for name, record in manifest.items():
        require((base / name).stat().st_size == record["bytes"] and
                sha(base / name) == record["sha256"], "evidence identity " + name)
    for item in data("owning-source-hashes.json")["sources"]:
        require((source / item["path"]).stat().st_size == item["bytes"] and
                sha(source / item["path"]) == item["sha256"], "current compiled source " + item["path"])
    for host in ["macos", "linux"]:
        gates = data(host + "-gates.json")
        require({g["gate"] for g in gates} == {"full", "capi", "all-targets", "integration"} and
                all(g["exit-code"] == 0 for g in gates), "portable gates " + host)
    gates = data(prefix + "gates.json")
    require({g["gate"] for g in gates} == {"full", "capi", "all-targets", "integration"} and
            all(g["exit-code"] == (101 if g["gate"] == "full" else 0) for g in gates),
            "Windows gates and recorded baseline failures")
    for name, count in [("macos-full.log", 7713), ("linux-full.log", 7707),
                        (prefix + "full.log", 7121)]:
        match = re.search(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;[^\n]*?(\d+) filtered out;", read(name))
        require(match and tuple(map(int, match.groups())) ==
                (count, 5 if name.startswith(prefix) else 0, 2, 0), "unfiltered full test counts " + name)
    failures = read(prefix + "full.log").split("failures:\n")[-1].split("test result:")[0]
    require({line.strip() for line in failures.splitlines() if line.strip()} == {
        "smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry",
        "smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry",
        "smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry",
        "smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards",
        "smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime"},
        "exact unchanged Windows baseline failure set")
    for name in ["macos-capi.log", "linux-capi.log", prefix + "capi.log"]:
        require("168 passed; 0 failed; 0 ignored" in read(name), "C API tests " + name)
    for name, count in [("macos-integration.log", 544), ("linux-integration.log", 544),
                        (prefix + "integration.log", 4)]:
        require(f"{count} passed; 0 failed; 0 ignored" in read(name), "native-selected integration " + name)
    require("25 passed; 0 failed; 0 ignored" in read("windows-directory-targeted.log") and
            "installed_directory_open_leaf_resolves_snapshot_and_ret12_cleanup ... ok" in
            read("windows-directory-targeted.log"), "installed Windows snapshot and leaf execution")
    require(all(g["exit-code"] == 0 for g in data("macos-owning-gates.json")) and
            all(g["exit-code"] == 0 for g in data("linux-owning-gates.json")), "Unix owning gates")
    archives = data(prefix + "owning-archive-hashes.json")
    require(archives["core-features"] == [] and len(archives["archives"]) == 2 and
            archives["sources"] == data("owning-source-hashes.json")["sources"], "actual owning source/core selection")
    for archive in archives["archives"]:
        require(archive["bytes"] > 0 and archive["members"] > 0 and
                archive["structural-member-walk"] == "pass" and
                re.fullmatch("[a-f0-9]{64}", archive["sha256"]), "actual complete archive walk")
    ordinary = data("ordinary-results.json")
    require(sha(base / (prefix + "after-trace.rs")) == ordinary["observer-sha256"] ==
            "d41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e",
            "unchanged ordinary observer identity")
    require(ordinary["previous-turn"] == 90924 and ordinary["current-turn"] == 91011 and
            ordinary["current-turn"] - ordinary["previous-turn"] == ordinary["scheduler-calls-advanced"] == 87,
            "ordinary scheduler arithmetic")
    require('terminal turn=91011 Complete(Internal("native NTDLL service NtOpenSymbolicLinkObject (0x138) at 0x1800023c0 is not implemented"))'
            in read(prefix + "after-production-trace-output.log"), "actual next native frontier")
    trace = read(prefix + "after-ordinary-processes.log")
    require([line[len("program "):] for line in trace.splitlines() if line.startswith("program ")] ==
            ordinary["programs"] and len(ordinary["programs"]) == 4 and trace.count("run reason=5") == 4 and
            trace.count("NtOpenSymbolicLinkObject (0x138)") == 4 and ordinary["guest-exits"] == 0,
            "four ordinary programs and no guest-exit claim")
    require(ordinary["archives"] == archives["archives"] and ordinary["source-units"] == 79 and
            ordinary["core-features"] == [], "ordinary owning artifact binding")
    for item in data("references/sources.json"):
        path = base / "references" / item["path"]
        require(path.stat().st_size == item["bytes"] and sha(path) == item["sha256"] and
                item["url"].startswith("https://learn.microsoft.com/"), "primary declaration identity")
print(json.dumps({"native-producers": 45, "mature-replay-rows": total,
                  "namespace-entries-per-profile": 151, "fresh-cases": 2,
                  "mode": "native-only" if native_only else "full", "result": "pass"}))
