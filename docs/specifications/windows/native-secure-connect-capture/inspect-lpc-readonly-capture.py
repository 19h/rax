"""Verify actual read-only capture producers before admitting their oracle."""
from pathlib import Path
import datetime
import hashlib
import json
import re
import struct
import sys

TASK = Path("C:/Windows/Temp/assist-native-rax-20261010")
SHARE = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
SOURCE = "15c627ae7087ebf9067d0cc0d68df91dfbc672ef9997dc96adc4bd9214937423"
PROFILES = {"arm64": (0xaa64, 8), "x64": (0x8664, 8), "x86": (0x14c, 4), "x86-laa": (0x14c, 4)}
NAMES = ["baseline", "output", "name-header", "name-text", "qos", "client-view",
         "sid", "server-view", "message", "information", "length"]


def require(value, message):
    if not value:
        raise ValueError(message)


def identity(path, pe=False):
    data = path.read_bytes()
    row = {"path": str(path), "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    if pe:
        require(data[:2] == b"MZ", "DOS header")
        at = struct.unpack_from("<I", data, 60)[0]
        require(data[at:at + 4] == b"PE\0\0", "PE header")
        row["machine"] = struct.unpack_from("<H", data, at + 4)[0]
        row["characteristics"] = struct.unpack_from("<H", data, at + 22)[0]
    return row


def validate(text, width):
    header = re.fullmatch(r"profile pid=(\d+) width=(\d+) cases=11", text.splitlines()[0])
    require(header and int(header[2]) == width, "actual process width")
    blocks = re.split(r"(?=^case=)", text, flags=re.M)[1:]
    require(len(blocks) == 11, "complete read-only partition")
    rows = []
    baseline_view = None
    for index, block in enumerate(blocks):
        row = dict(item.split("=", 1) for item in block.splitlines()[0].split())
        require(int(row["case"]) == index and row["name"] == NAMES[index], "exact case order")
        require(int(row["readonly-page"]) == index - 1, "one controlled read-only page")
        require(row["handles-before"] == row["handles-after"], "no native handle leak")
        require(row["status"] in {"C0000034", "C0000005", "DEADBEEF"}, "bounded negative observation")
        require((row["status"] == "DEADBEEF") == (row["exception"] != "00000000"), "NTSTATUS separated from caller SEH")
        captures = re.findall(r"^bytes name=(\S+) size=(\d+) hex=([0-9A-F]*)$", block, re.M)
        require(len(captures) == 6 and len({r[0] for r in captures}) == 6, "complete output captures")
        blobs = {name: bytes.fromhex(data) for name, size, data in captures}
        require(all(len(blobs[name]) == int(size) for name, size, data in captures), "exact captured spans")
        view = blobs["client-view"]
        require(len(view) == 48 and int.from_bytes(view[:4], "little") == 48 and
                view[4:8] == bytes(4) and int.from_bytes(view[8:16], "little") != 0 and
                view[16:24] == bytes(8) and int.from_bytes(view[24:32], "little") == 65536 and
                view[32:] == bytes(16), "actual owned pagefile section view")
        if baseline_view is None:
            baseline_view = view
        require(view == baseline_view, "failure client view unchanged")
        require(blobs["server-view"] == (24).to_bytes(4, "little") + bytes(20), "failure remote view unchanged")
        require(blobs["message"] == bytes([0xa5]) * 8 and blobs["information"] == bytes([0xa5]) * 64 and
                blobs["length"] == (32).to_bytes(4, "little") + bytes([0xa5]) * 4, "failure data outputs unchanged")
        output = blobs["output"]
        require(len(output) == 32 and output[4:] == bytes([0xa5]) * 28 and
                output[:4] in {bytes(4), bytes([0xa5]) * 4}, "bounded caller handle copyback")
        if width == 8:
            require(output == bytes([0xa5]) * 32, "native-width output unchanged")
        rows.append({**row, "output-prefix": output[:4].hex()})
    require(text.splitlines()[-1] == "complete cases=11 closed-handles=1 no-native-success=1", "explicit native cleanup")
    return rows


def main():
    require(__debug__, "optimized checks rejected")
    outcomes = json.loads((SHARE / "native-lpc-readonly-capture-outcomes.json").read_text(encoding="utf-8-sig"))
    require(len(outcomes) == 4 and {r["profile"] for r in outcomes} == set(PROFILES), "complete actual producers")
    producers = []
    for profile, (machine, width) in PROFILES.items():
        directory = TASK / ("lpc-readonly-capture-" + profile)
        stem = "native-lpc-readonly-capture-" + profile
        source = identity(directory / "native-lpc-readonly-capture.cpp")
        executable = identity(directory / (stem + ".exe"), True)
        require(source["sha256"] == SOURCE == identity(SHARE / "native-lpc-readonly-capture.cpp")["sha256"], "actual producer source")
        require(executable["machine"] == machine, "actual PE machine")
        if width == 4:
            require(bool(executable["characteristics"] & 0x20) == (profile == "x86-laa"), "actual LAA flag")
        outcome = next(r for r in outcomes if r["profile"] == profile)
        require(outcome["exit-code"] == 0 and outcome["source-hash"].lower() == SOURCE and
                outcome["exe-sha256"].lower() == executable["sha256"] and
                outcome["exe-bytes"] == executable["bytes"], "actual executable controller result")
        stdout = identity(directory / (stem + ".log"))
        require(stdout["sha256"] == identity(SHARE / (stem + ".log"))["sha256"], "byte-exact transferred stdout")
        rows = validate((directory / (stem + ".log")).read_text(), width)
        producers.append({"profile": profile, "source": source, "executable": executable, "stdout": stdout, "observations": rows})
    result = {"utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "kernel": list(sys.getwindowsversion()),
              "producers": producers, "observations": 44, "closed-handles": 4,
              "inspector": identity(Path(__file__)),
              "scope": "Missing private names only; compatibility ABIs on ARM64 kernel, physical x64 kernel unverified"}
    (SHARE / "native-lpc-readonly-capture-identities.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Verified 4 actual read-only producers, 44 observations, exact caller buffers and 4 closed handles")


if __name__ == "__main__":
    main()
