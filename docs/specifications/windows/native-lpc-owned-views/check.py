"""Verify primary native producer identities, ABI geometry and cleanup totals."""
from pathlib import Path
import hashlib
import json
import re

ROOT = Path(__file__).parent


def require(condition, message):
    if not condition:
        raise ValueError(message)


def verify(record):
    path = ROOT / record["path"].replace("\\", "/").rsplit("/", 1)[-1]
    data = path.read_bytes()
    require(len(data) == record["bytes"], str(path) + " byte count")
    require(hashlib.sha256(data).hexdigest() == record["sha256"], str(path) + " SHA-256")
    return data.decode("utf-8")


def main():
    require(__debug__, "optimized verification rejected")
    cases = closes = manual = replacements = 0
    for name, size, profiles in [
        ("native-lpc-view-lifetime2-identities.json", 48, {"arm64", "x64", "x86", "x86-laa"}),
        ("native-lpc-view32-lifetime2-identities.json", 24, {"x86", "x86-laa"}),
    ]:
        identities = json.loads((ROOT / name).read_text(encoding="utf-8-sig"))
        require(identities["kernel"][:3] == [10, 0, 29683], "kernel profile")
        require({p["profile"] for p in identities["producers"]} == profiles, "producer profiles")
        for producer in identities["producers"]:
            verify(producer["source"])
            require(len(producer["cases"]) == 5, "all five lifecycle variants")
            for case in producer["cases"]:
                text = verify(case["stdout"])
                require("connect status=00000000" in text, "successful native connection")
                require("server listen=00000000 accept=00000000 complete=00000000" in text, "actual peer handshake")
                require(f"bytes name=client-view-before size={size} " in text, "measured client ABI")
                require("maximum-message=256 information-length=32" in text, "controlled server geometry")
                require("closed name=section" in text and "closed name=client-port" in text and "closed name=server-peer" in text, "explicit closures")
                require(len(re.findall(r"^closed name=", text, re.M)) == case["closed-handles"], "actual close count")
                require("complete status=0" in text and "cleanup-unmaps=0 alias=1 reply=1" in text, "producer terminal checks")
                require(text.count("manual-unmap status=00000000") == case["manual-unmaps"], "manual unmap count")
                require(text.count("cleanup-private present=1 freed=1") == case["private-replacements-freed"], "private replacement cleanup")
                require(case["mapped-and-released-views"] == 2, "both endpoint views released")
                for side in ("client", "server"):
                    require(re.search(rf"^region phase=canary name={side} .*state=1000 protect=4 type=40000 read=1 got=2 data=6996", text, re.M), "real shared canary")
                cases += 1
                closes += case["closed-handles"]
                manual += case["manual-unmaps"]
                replacements += case["private-replacements-freed"]
    require((cases, closes, manual, replacements) == (30, 126, 12, 6), "complete native totals")
    print("Verified 30 native lifecycle cases;126 closed handles;60 released views;12 manual unmaps;6 preserved/freed private replacements")


if __name__ == "__main__":
    main()
