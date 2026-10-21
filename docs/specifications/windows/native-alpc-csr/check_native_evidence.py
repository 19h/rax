"""Verify native byte identities and measured CSR/ALPC contracts. Python3.9+."""

from pathlib import Path
import argparse, hashlib, json, os, re

PROFILES = ["arm64", "x64", "x86", "x86-laa"]
SLOTS = [
    8,
    16,
    32,
    40,
    56,
    88,
    104,
    168,
    184,
    200,
    216,
    232,
    248,
    280,
    296,
    312,
    552,
    568,
]


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def identity(path):
    h = hashlib.sha256()
    size = 0
    with path.open("rb") as stream:
        for b in iter(lambda: stream.read(1 << 20), b""):
            size += len(b)
            h.update(b)
    return {"bytes": size, "sha256": h.hexdigest()}


def pair(row):
    return {k: row[k] for k in ["bytes", "sha256"]}


def fields(line):
    return dict(re.findall(r"([\w-]+)=([^ ]*)", line))


def number(row, key):
    return int(row[key], 16)


def word(data, offset):
    return int.from_bytes(data[offset : offset + 8], "little")


def rows(lines, prefix):
    return [fields(line) for line in lines if line.startswith(prefix)]


def check_csr(lines, kind):
    calls = rows(lines, "CONNECT ")
    need(len(calls) == 2, "CSR sample extent")
    data = []
    for row in calls:
        need(
            row["server"] == "3"
            and row["supplied"] == "584"
            and row["returned"] == "584",
            "fixed USER query",
        )
        need(
            row["status"] == row["exception"] == "00000000" and row["inside"] == "0",
            "CSR success",
        )
        raw = bytes.fromhex(row["data"])
        need(len(raw) == 1024 and raw[584:] == bytes([0xA5]) * 440, "CSR output canary")
        data.append(raw[:584])
    need(data[0] == data[1], "repeated CSR metadata")
    summary = rows(lines, "SUMMARY ")
    need(
        len(summary) == 1
        and summary[0]["calls"] == "2"
        and summary[0]["handles-before"] == summary[0]["handles-after"],
        "CSR handle accounting",
    )
    if kind in ["mapping", "shared-export"]:
        region = rows(lines, "REGION sample=0 ")
        need(
            [int(x["slot"]) for x in region] == [8, 16, 32, 40, 56],
            "typed region observations",
        )
        for row in region:
            need(
                row["state"] == "00001000"
                and row["protect"] == "00000002"
                and row["type"] == "00040000",
                "readonly mapped pointer",
            )
            need(row["read"] == "1" and row["got"] == "64", "bounded pointer read")
    if kind == "mapping":
        maps = rows(lines, "MAP ")
        ends = rows(lines, "MAP-END ")
        need(len(maps) == 3 and len(ends) == 2, "selected allocation runs")
        admitted = []
        for slot, size, commit in [(16, 1 << 21, 8192), (40, 32768, 32768)]:
            selected = [x for x in maps if int(x["slot"]) == slot]
            end = [x for x in ends if int(x["slot"]) == slot]
            need(
                len(end) == 1
                and int(end[0]["bytes"]) == size
                and int(end[0]["committed"]) == commit,
                "mapping extent",
            )
            base = number(selected[0], "base")
            need(
                base == word(data[0], slot) and base % 65536 == 0,
                "mapping base identity",
            )
            cursor = 0
            measured_commit = 0
            for row in selected:
                span = int(row["bytes"])
                need(
                    int(row["offset"]) == cursor and span % 4096 == 0,
                    "mapping continuity",
                )
                need(row["type"] == "00040000", "mapped allocation type")
                if row["state"] == "00001000":
                    need(
                        row["protect"] == "00000002"
                        and row["stable"] == "1"
                        and len(bytes.fromhex(row["hex"])) == span,
                        "stable readonly snapshot",
                    )
                    admitted.append((base + cursor, base + cursor + span))
                    measured_commit += span
                else:
                    need(
                        row["state"] == "00002000"
                        and row["protect"] == "00000000"
                        and row["hex"] == "",
                        "reserved inaccessible run",
                    )
                cursor += span
            need(
                cursor == size and measured_commit == commit,
                "allocation cover/commitment",
            )
        need(
            word(data[0], 8) == word(data[0], 40) + 0x1040 and word(data[0], 24) == 32,
            "SHAREDINFO prefix",
        )
        for offset in SLOTS:
            value = word(data[0], offset)
            need(
                value == 0 or any(a <= value < b for a, b in admitted),
                "typed pointer outside committed data",
            )
    if kind == "shared-export":
        exports = rows(lines, "USER32 ")
        need(len(exports) == 1, "USER export observation")
        row = exports[0]
        need(
            row["shared-export"] == row["read"] == "1" and row["got"] == "80",
            "USER export read",
        )
        need(
            bytes.fromhex(row["hex"]) == data[0][8:88],
            "current gSharedInfo layout correlation",
        )
    return 2


def check_private(lines, kind, count, profile):
    if kind == "echo":
        call = rows(lines, "CALL ")
        server = rows(lines, "SERVER ")
        owned = rows(lines, "OWNED ")
        need(len(call) == len(server) == len(owned) == 1, "echo observations")
        need(call[0]["status"] == call[0]["exception"] == "00000000", "echo success")
        message = bytes.fromhex(call[0]["message"])
        header = 24 if profile.startswith("x86") else 40
        need(
            int(call[0]["capacity"]) == header + 48
            and message[4:6] == b"\x02\x00"
            and message[header] == 0x3A,
            "native/WoW echo conversion",
        )
        need(
            server[0]["received"] == "1"
            and owned[0]["port-handles-opened"]
            == owned[0]["port-handles-closed"]
            == "3",
            "owned echo cleanup",
        )
        return 1
    cases = rows(lines, "CASE ")
    expected = {
        "faults": set(range(40)),
        "priority": {0} | set(range(40, 72)),
        "alignment": {0} | set(range(40, 50)),
        "limits": {0} | set(range(40, 62)),
        "grant-priority": {0} | set(range(40, 51)),
    }[kind]
    need(
        len(cases) == count and {int(x["id"]) for x in cases} == expected,
        "private matrix coverage",
    )
    need(
        all(
            x["exception"] == "00000000" and x["last-error"] == "12345678"
            for x in cases
        ),
        "private exception/LastError",
    )
    owns = rows(lines, "OWNED ")
    ends = rows(lines, "END ")
    need(len(owns) == len(ends) == count, "private cleanup extent")
    for row in owns:
        need(
            row["port-handles-opened"] == row["port-handles-closed"]
            and row["server-thread-joined"] == "1",
            "owned handles/worker cleanup",
        )
    for row in ends:
        need(row["pages-opened"] == row["pages-released"] == "3", "owned page cleanup")
    by = {int(x["id"]): x for x in cases}
    wow = profile.startswith("x86")
    if kind == "faults":
        for c, status in {
            1: "C00000F0",
            2: "C0000005",
            4: "C0000705",
            5: "80000002",
            6: "80000002",
            8: "C0000005",
            20: "C0000008",
            21: "C0000024",
            22: "C0000024",
            25: "C00000F0",
            27: "C0000005",
            28: "C0000005",
            31: "C0000005",
            32: "80000001",
            33: "80000001",
            34: "80000001",
            37: "80000001",
        }.items():
            need(by[c]["status"] == status, "kernel fault status " + str(c))
        for c in [16, 18, 19]:
            need(
                by[c]["status"]
                == ("00000000" if wow else ("C000002F" if c == 19 else "C000000D")),
                "WoW total-word conversion",
            )
        need(
            by[37]["receive-after"] == "00000002", "readonly receiver guard visitation"
        )
    elif kind == "priority":
        need(
            by[42]["status"] == ("C0000005" if wow else "C0000008"),
            "WoW length-before-handle",
        )
        need(
            by[51]["status"] == ("C0000005" if wow else "80000002"),
            "length/alignment order",
        )
        need(
            by[69]["send-after"] == "00000104"
            and by[69]["receive-after"] == "00000004",
            "receiver-before-sender guard",
        )
        need(
            by[71]["status"] == ("80000001" if wow else "C0000005"),
            "timeout/length order",
        )
    elif kind == "alignment":
        need(
            by[40]["status"] == "80000002"
            and all(
                by[c]["status"] == "00000000" for c in [41, 42, 43, 44, 45, 46, 47]
            ),
            "ULONG receiver alignment and unaligned input capture",
        )
    elif kind == "limits":
        need(
            all(by[c]["status"] == "C0000022" for c in [53, 55, 56, 58]),
            "PORT_CONNECT required",
        )
        need(
            all(by[c]["status"] == "00000000" for c in [54, 57, 59, 60, 61]),
            "connect-only/full/tagged handles",
        )
        need(
            by[48]["status"] == by[49]["status"] == "C000000D",
            "16bit wrapped native total",
        )
    elif kind == "grant-priority":
        grants = rows(lines, "GRANT ")
        need(len(grants) == count, "queried-grant extent")
        for row in grants:
            c = int(row["case"])
            need(
                row["status"] == "00000000"
                and row["supplied"] == row["returned"] == "56",
                "SDK basic query",
            )
            need(
                number(row, "granted") == (0x1F0001 if c == 0 else 1 if c == 50 else 0),
                "actual granted-access value",
            )
        for c in [40, 41, 43, 44, 45, 47, 49]:
            need(by[c]["status"] == "C0000022", "access check before pointer/flags")
        need(by[46]["status"] == "C0000024", "object type before access")
        need(
            by[42]["status"] == ("C0000005" if wow else "C0000022"),
            "thunk length before granted-access",
        )
        need(
            by[48]["status"] == ("80000001" if wow else "C0000022"),
            "granted-access guard order",
        )
        need(
            by[50]["status"] == ("80000001" if wow else "C0000005"),
            "timeout before native length",
        )
    return len(cases)


def check_trace(lines):
    send = next(
        bytes.fromhex(fields(x)["hex"])
        for x in lines
        if x.startswith("dump name=send-message ")
    )
    reply = next(
        bytes.fromhex(fields(x)["hex"])
        for x in lines
        if x.startswith("dump name=receive-after ")
    )
    need(
        len(send) == len(reply) == 88 and send[:4] == reply[:4] == b"\x30\x00\x58\x00",
        "native wire lengths",
    )
    need(
        reply[4:6] == b"\x02\x00"
        and word(reply, 48) == 0
        and word(reply, 64) == 3
        and word(reply, 80) == 584,
        "native reply type/API/status",
    )
    alias = fields(next(x for x in lines if x.startswith("client-alias ")))
    remote = number(alias, "remote")
    offset = word(send, 40) - remote
    before = next(
        bytes.fromhex(fields(x)["hex"])
        for x in lines
        if x.startswith("dump name=client-section-before-call ")
    )
    after = [
        bytes.fromhex(fields(x)["hex"])
        for x in lines
        if x.startswith("dump name=client-section-after-call ")
    ][-1]
    need(offset >= 0 and offset + 628 <= len(before), "owned capture boundary")
    need(word(send, 72) == word(send, 40) + 40, "capture information offset")
    need(
        int.from_bytes(before[offset : offset + 4], "little") == 628
        and int.from_bytes(before[offset + 16 : offset + 20], "little") == 1
        and word(before, offset + 32) == 72,
        "native capture schema",
    )
    need(
        before[offset + 40 : offset + 624] == bytes(584)
        and after[offset + 8 : offset + 16] == bytes(8),
        "native related-buffer transition",
    )
    child = fields(next(x for x in lines if x.startswith("child-connect ")))
    need(
        child["status"] == "00000000"
        and child["inside"] == "0"
        and after[offset + 40 : offset + 624] == bytes.fromhex(child["data"]),
        "CSR captured USER reply",
    )
    need(
        word(reply, 72) == word(send, 72) and word(reply, 40) != word(send, 40),
        "reply shared pointer and opaque capture value",
    )
    need(
        any(x == "receive-length before=952 after=88" for x in lines),
        "native receive length",
    )
    need(
        any("entered=2 returned=2 exit=41" in x for x in lines),
        "native child completion",
    )
    return 1


def check(directory, source=None, live=False):
    manifest = json.loads((directory / "native-producers.json").read_text())
    need(len(manifest["accepted"]) == 11, "producer set")
    native = directory / "native"
    files = {}
    calls = queries = images = 0
    for p in manifest["accepted"]:
        launch = native / p["launch"]
        need(
            identity(launch)
            == {"bytes": p["launch-bytes"], "sha256": p["launch-sha256"]},
            "launch identity",
        )
        r = json.loads(launch.read_text())
        need(
            r["state"] == "finished" and r["exit-code"] == 0,
            "native producer incomplete",
        )
        need(
            r["source-before"] == r["source-after"]
            and identity(native / p["source"]) == pair(r["source-before"])
            and p["source-sha256"] == r["source-before"]["sha256"],
            "native source binding",
        )
        need(
            r["runtime-modules-before"] == r["runtime-modules-after"],
            "runtime DLL stability",
        )
        need(
            r["compiler-environment"]["sha256"]
            == "e6ca43bcdcbd3067b5de05d051259d4d3abf0bfa5283a98daac14fd508a05fad",
            "MSVC environment",
        )
        for row in [r["source-after"], *r["runtime-modules-after"]]:
            if row["path"] in files:
                need(
                    pair(files[row["path"]]) == pair(row),
                    "cross-producer live identity",
                )
            files[row["path"]] = row
        for step in r["steps"]:
            need(
                step["exit-code"] == 0 and identity(native / step["log"]) == pair(step),
                "native stage identity",
            )
        need(
            [x["profile"] for x in r["outcomes"]] == p["profiles"]
            and p["profiles"] == (["arm64"] if p["kind"] == "trace" else PROFILES),
            "execution-view coverage",
        )
        for row in r["outcomes"]:
            profile = row["profile"]
            need(
                row["exit-code"] == 0
                and row["executable-before"] == row["executable-after"],
                "executed image stability",
            )
            exe = row["executable-after"]
            files[exe["path"]] = exe
            images += 1
            prefix = p["prefix"] + "-four-views-" + profile
            before = json.loads((native / (prefix + "-before.log")).read_text())
            after = json.loads((native / (prefix + "-after.log")).read_text())
            need(before == after and pair(before) == pair(exe), "PE identity")
            need(
                before["machine"]
                == {"arm64": 0xAA64, "x64": 0x8664, "x86": 0x14C, "x86-laa": 0x14C}[
                    profile
                ],
                "PE machine",
            )
            need(profile != "x86-laa" or before["large-address-aware"], "LAA execution")
            lines = (native / row["log"]).read_text().splitlines()
            if p["kind"] in ["csr", "mapping", "shared-export"]:
                queries += check_csr(lines, p["kind"])
            elif p["kind"] == "trace":
                queries += check_trace(lines)
            else:
                calls += check_private(lines, p["kind"], p["calls-per-view"], profile)
    need(
        (images, calls, queries) == (41, 480, 33), "accepted native observation extent"
    )
    for rejected in manifest["rejected-controls"]:
        for row in rejected["files"]:
            need(
                identity(native / row["path"]) == pair(row),
                "rejected-control provenance",
            )
    if source:
        inputs = json.loads(
            (directory / "validation/alpc-candidate2-source.json").read_text()
        )["files"]
        for row in inputs:
            path = source / row["path"]
            expected = pair(row)
            if os.name == "nt" and row["path"] == ".cargo/config.toml":
                expected = {
                    "bytes": 374,
                    "sha256": "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
                }
            raw = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
            need(
                {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}
                == expected,
                "compiled source " + row["path"],
            )
    if live:
        for row in files.values():
            need(
                identity(Path(row["path"])) == pair(row),
                "live native input " + row["path"],
            )
    return {
        "state": "verified",
        "native-executables": images,
        "private-ALPC-calls": calls,
        "successful-USER-queries": queries,
        "unique-live-inputs": len(files),
        "source-inputs": len(inputs) if source else 0,
        "kernel-scope": manifest["kernel-scope"],
    }


if __name__ == "__main__":
    a = argparse.ArgumentParser()
    a.add_argument("--source", type=Path)
    a.add_argument("--live-native", action="store_true")
    a.add_argument("--output", type=Path)
    args = a.parse_args()
    result = check(Path(__file__).resolve().parent, args.source, args.live_native)
    text = json.dumps(result, indent=2) + "\n"
    print(text, end="")
    if args.output:
        args.output.write_text(text)
