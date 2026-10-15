from pathlib import Path
import datetime
import hashlib
import json
import re
import struct
import sys

TASK = Path("C:/Windows/Temp/assist-native-rax-20261010")
SHARE = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")


def require(ok, message):
    if not ok:
        raise ValueError(message)


def identity(path, executable=False):
    data = path.read_bytes()
    row = {"path": str(path), "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    if executable:
        require(data[:2] == b"MZ", "DOS signature")
        offset = struct.unpack_from("<I", data, 0x3c)[0]
        require(data[offset:offset+4] == b"PE\0\0", "PE signature")
        row["machine"] = struct.unpack_from("<H", data, offset+4)[0]
        row["characteristics"] = struct.unpack_from("<H", data, offset+22)[0]
    return row


require(__debug__, "optimized inspection rejected")
mode = sys.argv[1]
require(mode in ["first", "arm64", "complete"], "inspection selection")
producers = [("secure-connect-first-call", "arm64", "d5231391dbd2b68ee1c7359be933e8a4f7cb288713e63185edbd0d13362e0a04")]
if mode in ["arm64", "complete"]:
    producers += [("secure-connect-first-call2", profile, "8d9fb463dd33618e8e1b2e1f0358d952e604220044f463bee96237c8417bb8d3") for profile in (["arm64"] if mode == "arm64" else ["arm64", "x64", "x86", "x86-laa"])]
rows = []
for kind, profile, expected_source in producers:
    directory = TASK / (kind + "-" + profile)
    stem = "native-" + kind + "-" + profile
    source = identity(directory / ("native-" + kind + ".cpp"))
    executable = identity(directory / (stem + ".exe"), True)
    stdout = identity(directory / (stem + ".log"))
    outcome_name = "native-" + kind + ("-compat" if profile != "arm64" else "") + "-outcomes.json"
    outcomes = json.loads((SHARE / outcome_name).read_text(encoding="utf-8-sig"))
    outcomes = outcomes if isinstance(outcomes, list) else [outcomes]
    outcome = [item for item in outcomes if item["profile"] == profile]
    require(len(outcome) == 1, "unique producer outcome")
    outcome = outcome[0]
    require(source["sha256"] == expected_source == outcome["source-hash"].lower(), "actual compiled source")
    require(executable["machine"] == {"arm64": 0xaa64, "x64": 0x8664, "x86": 0x14c, "x86-laa": 0x14c}[profile], "PE machine")
    if profile.startswith("x86"):
        require(bool(executable["characteristics"] & 0x20) == (profile == "x86-laa"), "x86 LAA")
    require(executable["sha256"] == outcome["exe-sha256"].lower() and executable["bytes"] == outcome["exe-bytes"] and outcome["exit-code"] == 0, "actual executable outcome")
    require(stdout["sha256"] == identity(SHARE / (stem + ".log"))["sha256"], "raw actual stdout transfer")
    text = (directory / (stem + ".log")).read_text()
    entries = re.findall(r"^entry ordinal=(\d+) caller=[0-9A-F]+ args=([0-9A-F,]+)$", text, re.M)
    require(len(entries) == 1 and len(entries[0][1].rstrip(",").split(",")) == 9, "original nine arguments")
    require("return ordinal=0 status=00000000" in text and text.splitlines()[-1] == "complete captured=1 calls=1 exit=41", "original status and natural child exit")
    name = re.findall(r"^dump name=port-name-utf16 .*hex=([0-9A-F]+)$", text, re.M)
    require(len(name) == 1 and bytes.fromhex(name[0]).decode("utf-16-le") == "\\Windows\\ApiPort", "actual CSR port name")
    rows.append({"kind": kind, "profile": profile, "source": source, "executable": executable, "stdout": stdout, "native-child-exit": 41})
record = {"recorded-utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "kernel": list(sys.getwindowsversion()), "inspector": identity(Path(__file__)), "producers": rows, "ordinary-emulated-exits": 0}
(SHARE / ("native-secure-connect-" + mode + "-identities.json")).write_text(json.dumps(record, indent=2) + "\n")
print("Verified", len(rows), "actual native producers; original nine-argument CSR calls,unmodified status,natural owned child exit41")
