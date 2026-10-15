"""Bind actual native producers to complete missing-endpoint observations."""
from pathlib import Path
import collections
import datetime
import hashlib
import json
import re
import struct
import sys

TASK = Path("C:/Windows/Temp/assist-native-rax-20261010")
SHARE = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
SOURCE = "7d66d6952abe20a8762bfb68aaec5e464276be0736681e4ce8e60b9468ebcb65"
PROFILES = {"arm64": (0xaa64, 8), "x64": (0x8664, 8), "x86": (0x14c, 4), "x86-laa": (0x14c, 4)}


def require(value, message):
    if not value:
        raise ValueError(message)


def identity(path, pe=False):
    data = path.read_bytes()
    row = {"path": str(path), "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    if pe:
        require(data[:2] == b"MZ", "DOS signature")
        at = struct.unpack_from("<I", data, 60)[0]
        require(data[at:at + 4] == b"PE\0\0", "PE signature")
        row["machine"] = struct.unpack_from("<H", data, at + 4)[0]
        row["characteristics"] = struct.unpack_from("<H", data, at + 22)[0]
    return row


def expected_status(row, width):
    name = row["name"]
    faults = {"output-1", "output-2", "output-4", "output-5", "name-2", "name-4",
              "qos-pointer-2", "client-pointer-2", "server-pointer-2", "sid-2",
              "message-pointer-2", "message-pointer-4", "length-pointer-2", "length-pointer-4",
              "data-pointer-1", "data-pointer-2", "data-pointer-3"}
    if name in faults or (name.startswith("priority-") and name != "priority-0"):
        return "C0000005"
    if name in {"name-1", "name-3", "name-7"}:
        return "C0000033"
    if name == "name-5":
        return "80000002"
    if name == "name-8":
        return "C000003A"
    if name in {"sid-3", "sid-4"}:
        return "C0000078"
    if name.startswith(("client-length-", "server-length-")):
        if name == "client-length-24" and width == 4:
            return "C0000034"
        return "C000000D"
    return "C0000034"


def validate(text, width):
    header = re.fullmatch(r"profile pid=(\d+) width=(\d+) cases=88", text.splitlines()[0])
    require(header and int(header[2]) == width, "actual native width and case count")
    blocks = re.split(r"(?=^input case=)", text, flags=re.M)[1:]
    require(len(blocks) == 88, "complete native case partition")
    rows = []
    names = set()
    counts = collections.Counter()
    early = {"output-1", "output-2", "output-4", "output-5", "name-2",
             "qos-pointer-2", "client-pointer-2", "server-pointer-2"}
    for index, block in enumerate(blocks):
        lines = re.findall(r"^case=.*$", block, re.M)
        require(len(lines) == 1, "one response per captured input")
        row = dict(item.split("=", 1) for item in lines[0].split())
        require(int(row["case"]) == index and row["name"] not in names, "ordered distinct cases")
        names.add(row["name"])
        require(row["exception"] == "00000000", "NTSTATUS distinct from caller SEH")
        require(row["status"] == expected_status(row, width), "observed status class " + row["name"])
        require(row["handles-before"] == row["handles-after"], "unchanged actual handle count")
        captures = re.findall(r"^bytes name=(\S+) size=(\d+) hex=([0-9A-F]*)$", block, re.M)
        require(len(captures) == 12 and len({v[0] for v in captures}) == 12, "complete input/output captures")
        blobs = {name: bytes.fromhex(data) for name, size, data in captures}
        require(all(len(blobs[name]) == int(size) for name, size, data in captures), "complete byte spans")
        require(len(blobs["input-client-view"]) == 48 and len(blobs["input-server-view"]) == 24,
                "explicit 64-bit view fixture, including WoW conversion cases")
        require(blobs["client-view"] == blobs["input-client-view"] and
                blobs["server-view"] == blobs["input-server-view"], "failure view buffers unchanged")
        view = blobs["input-client-view"]
        require(int.from_bytes(view[:4], "little") == int(row["client-length"]) and
                int.from_bytes(view[8:16], "little") == int(row["section"], 16) and
                int.from_bytes(view[16:20], "little") == int(row["offset"]) and
                int.from_bytes(view[24:32], "little") == int(row["view-size"]), "captured view fields")
        qos = struct.pack("<IIBB2x", int(row["qos-length"]), int(row["impersonation"]),
                          int(row["tracking"]), int(row["effective"]))
        require(blobs["input-qos"] == qos, "actual QoS bytes")
        sid = bytearray(96)
        sid[0], sid[1], sid[7], sid[8] = 1, 1, 5, 18
        if row["sid"] == "3": sid[0] = 2
        if row["sid"] == "4": sid[1] = 16
        if row["sid"] == "5": sid[1] = 0
        require(blobs["input-sid"] == sid, "actual bounded SID bytes")
        name = blobs["input-name-text"].decode("utf-16le")
        require(re.fullmatch(r"\\Windows\\RaxLpcMissing-" + header[1] + r"-[0-9A-F]+", name,
                             re.I if row["name-mode"] == "8" else 0), "unique missing owned name")
        before = re.search(r"^input case=(\d+) name-length=(\d+) name-maximum=(\d+) args=([0-9A-F,]+)$", block, re.M)
        require(before and int(before[1]) == index and len(before[4].rstrip(",").split(",")) == 9, "nine actual arguments")
        require(len(blobs["input-name-header"]) == (16 if width == 8 else 8), "native UNICODE_STRING layout")
        require(struct.unpack_from("<HH", blobs["input-name-header"]) == (int(before[2]), int(before[3])), "captured string bounds")
        arena = bytearray([0xa5]) * (8 * 4096)
        where = 4 * 4096 + (4096 - 2 if row["length-pointer"] == "4" else 64 + (row["length-pointer"] == "3"))
        arena[where:where + 4] = int(row["length"]).to_bytes(4, "little")
        skipped = row["name"] in early or (row["name"].startswith("priority-") and row["name"] != "priority-0")
        if width == 4 and not skipped:
            where_out = 64 + (row["out"] == "3")
            arena[where_out:where_out + 4] = bytes(4)
        for name, page, selector in [("output", 0, "out"), ("message", 2, "message"),
                                     ("length", 4, "length-pointer"), ("information", 6, "data")]:
            crossed = row[selector] == ("3" if name == "information" else "4")
            offset = page * 4096 + (4096 - 32 if crossed else 64)
            require(blobs[name] == arena[offset:offset + 64], "exact failure copyback " + name + " " + row["name"])
        counts[row["status"]] += 1
        rows.append({"case": index, "name": row["name"], "status": row["status"], "caller-exception": row["exception"]})
    require(text.splitlines()[-1] == "complete cases=88 closed-handles=2 no-native-success=1", "owned cleanup")
    require(counts == ({"C0000034": 47 if width == 8 else 48, "C0000005": 24,
                        "C000000D": 10 if width == 8 else 9, "C0000033": 3,
                        "C0000078": 2, "80000002": 1, "C000003A": 1}), "complete status accounting")
    return {"cases": rows, "status-counts": dict(counts)}


def main():
    require(__debug__, "optimized checks rejected")
    outcomes = json.loads((SHARE / "native-lpc-missing-controls-outcomes.json").read_text(encoding="utf-8-sig"))
    require(len(outcomes) == 4 and {x["profile"] for x in outcomes} == set(PROFILES), "exact native producers")
    producers = []
    for profile, (machine, width) in PROFILES.items():
        directory = TASK / ("lpc-missing-controls-" + profile)
        stem = "native-lpc-missing-controls-" + profile
        source = identity(directory / "native-lpc-missing-controls.cpp")
        executable = identity(directory / (stem + ".exe"), True)
        outcome = next(x for x in outcomes if x["profile"] == profile)
        require(source["sha256"] == SOURCE == identity(SHARE / "native-lpc-missing-controls.cpp")["sha256"], "actual producer source")
        require(executable["machine"] == machine, "actual producer PE machine")
        if width == 4:
            require(bool(executable["characteristics"] & 0x20) == (profile == "x86-laa"), "actual LAA flag")
        require(outcome["exit-code"] == 0 and outcome["source-hash"].lower() == SOURCE and
                outcome["exe-sha256"].lower() == executable["sha256"] and outcome["exe-bytes"] == executable["bytes"], "actual controller outcome")
        stdout = identity(directory / (stem + ".log"))
        require(stdout["sha256"] == identity(SHARE / (stem + ".log"))["sha256"], "byte-exact stdout transfer")
        producers.append({"profile": profile, "source": source, "executable": executable, "stdout": stdout,
                          **validate((directory / (stem + ".log")).read_text(), width)})
    result = {"utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "kernel": list(sys.getwindowsversion()),
              "producers": producers, "inspector": identity(Path(__file__)), "observations": 352,
              "closed-handles": 8, "caller-exceptions": 0,
              "scope": "Unique missing endpoints only; no host service IPC; compatibility ABIs measured on ARM64 kernel; physical x64 kernel unknown"}
    (SHARE / "native-lpc-missing-controls-identities.json").write_text(json.dumps(result, indent=2) + "\n")
    print("Verified 4 actual producers, 352 native negative observations, exact failure copyback, 0 caller exceptions and 8 closed handles")


if __name__ == "__main__":
    main()
