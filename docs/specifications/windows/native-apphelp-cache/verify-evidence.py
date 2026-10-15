"""Verify the native producers, replay corpus and exact declared file bytes."""
from pathlib import Path
import hashlib
import json
import re
import struct

BASE = Path(__file__).resolve().parent


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fields(line):
    return dict(re.findall(r"([\w-]+)=([^ ]*)", line))


def verify(base):
    identities = json.loads((base / "native-apphelp-identities.json").read_text())
    profiles = ["arm64", "x64", "x86", "x86-laa"]
    groups = ["lookup", "offsets", "output-offsets", "names", "cdb-names", "buffers", "ordering"]
    producers = identities["producers"]
    require([(r["group"], r["profile"]) for r in producers] == [(g, p) for g in groups for p in profiles], "exact native producer matrix")
    require(identities["native-machine"] == 0xAA64 and identities["process-machine"] == 0, "native inspector architecture")
    require(identities["kernel"][:3] == [10, 0, 29683], "measured kernel version")
    flags = json.loads((base / "cdb-candidate-flags.json").read_text())["names"]
    require(len(flags) == 167, "167 original database candidates")
    totals = {}
    input_bytes = 0
    output_bytes = 0
    for row in producers:
        group, profile = row["group"], row["profile"]
        stem = f"native-apphelp-{group}-{profile}"
        source = base / f"native-apphelp-{group}.cpp"
        stdout = base / f"{stem}.log"
        for path, identity in [(source, row["source"]), (stdout, row["stdout"])]:
            require(path.stat().st_size == identity["bytes"] and digest(path) == identity["sha256"], "native file identity: " + path.name)
        outcomes = json.loads((base / f"native-apphelp-{group}-outcomes.json").read_text(encoding="utf-8-sig"))
        outcome = next(item for item in outcomes if item["profile"] == profile)
        require(outcome["exit-code"] == 0 and outcome["source-hash"].lower() == row["source"]["sha256"], "actual executed source")
        require(outcome["exe-sha256"].lower() == row["executable"]["sha256"] and outcome["exe-bytes"] == row["executable"]["bytes"], "actual executed binary")
        executable = row["executable"]
        require(executable["machine"] == {"arm64": 0xAA64, "x64": 0x8664, "x86": 0x14C, "x86-laa": 0x14C}[profile], "producer machine")
        require(bool(executable["characteristics"] & 0x20) == (profile != "x86"), "producer LAA bit")
        text = stdout.read_text()
        rows = [fields(line) for line in text.splitlines() if line.startswith("case=")]
        expected = {"lookup": 16, "offsets": 15 if profile.startswith("x86") else 11, "output-offsets": 210 if profile.startswith("x86") else 11, "names": 25, "cdb-names": 167, "buffers": 101, "ordering": 36}[group]
        require([int(item["case"]) for item in rows] == list(range(expected)) and row["cases"] == expected, "complete ordered native cases")
        require(text.splitlines()[-1].startswith("complete") and "failure=" not in text, "complete native output")
        require(all(item.get("escaped", "00000000") == "00000000" for item in rows), "no escaped native exceptions")
        totals[group] = totals.get(group, 0) + expected
        if group == "cdb-names":
            observed = {}
            for item in rows:
                name = bytes.fromhex(item["name-utf16"]).decode("utf-16-be")
                require(item["status"] == "00000000" and item["changes"] == "", "native counted-name result")
                observed[name] = int(item["value"], 16)
            require(observed == flags, "native flags equal independent database candidates")
        if group in ("buffers", "ordering"):
            for item in rows:
                data = item["input-readable"]
                output = item["output-bytes"]
                require(len(data) == 1024 and len(output) == 16, "fixed native byte capture")
                input_bytes += sum(data[i:i + 2] != "--" for i in range(0, len(data), 2))
                output_bytes += sum(output[i:i + 2] != "--" for i in range(0, len(output), 2))
                require(len(item["protections"].strip(",").split(",")) == 8, "native page protection capture")
    require(sum(totals.values()) == 1874 == identities["total-cases"], "native call total")
    require(sum(totals[name] for name in ["names", "cdb-names", "buffers", "ordering"]) == 1316, "replayed call total")
    require((input_bytes, output_bytes) == (261076, 3724), "readable native byte comparisons")
    files = json.loads((base / "sdb-file-structure.json").read_text())
    require(len(files) == 2, "two independent installed file catalogs")
    for file in files:
        require(file["candidates"] == flags and not file["unresolved"] and file["validation-error"] is None, "complete installed file parser match")
    controls = json.loads((base / "native-sdb-padding.json").read_text())["records"]
    require(len(controls) == 6, "six synthetic installed-reader controls")
    for control in controls:
        if control["padded"]:
            require(control["next-tag"] == 0x4032, "padded next-tag recognition")
        else:
            require(control.get("open") == "rejected" or control.get("next-tag") != 0x4032, "unpadded format rejected")
    if (base / "evidence-manifest.json").exists():
        manifest = json.loads((base / "evidence-manifest.json").read_text())
        for name, identity in manifest["inputs"].items():
            relative = Path(name)
            require(not relative.is_absolute() and ".." not in relative.parts, "relative evidence name")
            path = base / relative
            require(path.stat().st_size == identity["bytes"] and digest(path) == identity["sha256"], "frozen evidence: " + name)
    return {"native-producers": 28, "native-calls": 1874, "raw-replays": 1316, "readable-input-bytes": input_bytes, "readable-output-bytes": output_bytes, "padding-controls": 6}


if __name__ == "__main__":
    require(__debug__, "optimized validation is rejected")
    print(json.dumps(verify(BASE), indent=2))
