"""Check original native source/output identities and mapping/copyback evidence."""
from pathlib import Path
from collections import Counter
import hashlib
import json
import re

ROOT = Path(__file__).parent


def require(ok, message):
    if not ok:
        raise ValueError(message)


def verify(row):
    name = row["path"].replace("\\", "/").rsplit("/", 1)[-1]
    data = (ROOT / name).read_bytes()
    require((len(data), hashlib.sha256(data).hexdigest()) ==
            (row["bytes"], row["sha256"]), "actual native identity " + name)
    return data.decode("utf-8").replace("\r\n", "\n")


def main():
    require(__debug__, "optimized verification rejected")
    identities = json.loads((ROOT / "native-nls-initialize-identities.json").read_text())
    require(identities["kernel"][:3] == [10, 0, 29683], "native kernel identity")
    require(len(identities["producers"]) == 4, "four actual native profiles")
    total, retained, mappings, closes = 0, 0, 0, 0
    for producer in identities["producers"]:
        verify(producer["source"])
        text = verify(producer["stdout"])
        profile = producer["profile"]
        require(producer["machine"] == {"arm64": 0xAA64, "x64": 0x8664, "x86": 0x14C, "x86-laa": 0x14C}[profile], "PE machine " + profile)
        rows = re.findall(r"^case ordinal=(\d+) .*?status=([A-F0-9]{8}) seh=([A-F0-9]{8}).*\n"
                          r"guard protect=([A-F0-9]{8})\nlocale-views before=(\d+) after=(\d+)([^\n]*)\noutputs hex=([A-F0-9]+)$", text, re.M)
        require(len(rows) == 79, "complete native cases " + profile)
        require([int(row[0]) for row in rows] == list(range(79)), "contiguous native ordinals")
        statuses = Counter(row[1] for row in rows)
        require(statuses == {"00000000": 35, "C0000005": 37, "80000001": 7}, "native status counts " + profile)
        require(all(row[2] == "00000000" and len(row[7]) == 288 for row in rows), "no caller SEH and exact output bytes")
        lost = 0
        for ordinal, status, seh, guard, before, after, new, output in rows:
            delta = int(after) - int(before)
            require(delta in [0, 1] and len(re.findall(r"new=[A-F0-9]+", new)) == delta, "bounded actual new view")
            mappings += delta
            if status != "00000000" and delta:
                require(profile.startswith("x86") and int(ordinal) in [3, 6, 46, 51], "measured late WoW64 view retention")
                data = bytes.fromhex(output)
                require(data[32:36] == bytes.fromhex("09040000"), "locale survives late pointer fault")
                lost += 1
        require(lost == (4 if profile.startswith("x86") else 0), "exact retained failed-view count")
        retained += lost
        count = 35 if profile.startswith("x86") else 34
        require(text.splitlines()[-1] == f"complete cases=79 success=35 failed=44 seh=0 unmapped={count}", "native terminal record")
        require(len(re.findall(r"^unmap status=00000000$", text, re.M)) == count, "actual successful unmaps")
        require("filename=\\Device\\HarddiskVolume3\\Windows\\System32\\locale.nls" in text, "mapped installed file identity")
        total += len(rows)
        closes += count
    for row in json.loads((ROOT / "references/provenance.json").read_text()):
        require(hashlib.sha256((ROOT / "references" / row["file"]).read_bytes()).hexdigest() == row["sha256"], "pinned PHNT reference")
    require((total, retained, mappings, closes) == (316, 8, 148, 138), "all original observations and ownership counts")
    protection = json.loads((ROOT / "native-nls-initialize-protection-identities.json").read_text())
    require(len(protection["producers"]) == 4, "four protection producers")
    for producer in protection["producers"]:
        verify(producer["source"])
        text = verify(producer["stdout"])
        rows = re.findall(r"^case ordinal=(\d+) operation=(\S+) requested=([A-F0-9]+) status=([A-F0-9]+).*old=([A-F0-9]+) state=([A-F0-9]+) protection=([A-F0-9]+) type=([A-F0-9]+)$", text, re.M)
        require(len(rows) == 12 and [int(row[0]) for row in rows] == list(range(12)), "twelve original protection/free controls")
        for ordinal, operation, requested, status, old, state, protect, kind in rows:
            expected = "00000000" if int(ordinal) == 1 else "C0000045" if int(ordinal) < 10 else "C000001B"
            require(status == expected and (state, protect, kind) == ("00001000", "00000002", "00040000"), "fixed native initialization view")
            require(old == ("00000002" if int(ordinal) == 1 else "00000001" if int(ordinal) < 10 else "A5A5A5A5"), "native protect old-output copyback")
        require(text.count("unmap status=00000000\n") == 12, "twelve protection view closes")
    leaves = json.loads((ROOT / "native-nls-initialize-leaf-identities.json").read_text())
    require(len(leaves["producers"]) == 4, "four original leaf readers")
    for producer in leaves["producers"]:
        verify(producer["source"])
        text = verify(producer["stdout"])
        rows = re.findall(r"^leaf name=(\S+) pointer-bytes=(\d+) hex=([A-F0-9]{64})$", text, re.M)
        require(len(rows) == 3, "three original installed leaves")
        if producer["profile"].startswith("x86"):
            name, width, encoded = rows[0]
            data = bytes.fromhex(encoded)
            require(name == "NtInitializeNlsFiles" and width == "4" and data[12:15] == b"\xc2\x0c\x00", "actual x86 RET12 overrides legacy PHNT prototype")
    startup = json.loads((ROOT / "native-nls-initialize-first-call-identities.json").read_text())
    require(len(startup["producers"]) == 1, "original startup observer")
    producer = startup["producers"][0]
    verify(producer["source"])
    text = verify(producer["stdout"])
    require("return ordinal=0 name=NtInitializeNlsFiles status=00000000" in text and
            "bytes=864256 state=00001000 protect=00000002 type=00040000" in text and
            text.splitlines()[-1] == "complete installed=1 entered=1 returned=1 exit=41", "natural native initialization startup")
    print("Verified316nativecalls,48protection/freecontrols,12installedleafcaptures,actualx86RET12,andoriginalnativeNLSstartup")


if __name__ == "__main__":
    main()
