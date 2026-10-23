from pathlib import Path
import datetime, hashlib, json, struct, traceback

S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
OUT = S / "native-win32u-kernel-exports-v3.json"
assert not OUT.exists()
result = {
    "state": "starting",
    "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "images": [],
}


def image(path):
    raw = path.read_bytes()
    assert len(raw) <= 64 << 20 and raw[:2] == b"MZ"
    u16 = lambda p: struct.unpack_from("<H", raw, p)[0]
    u32 = lambda p: struct.unpack_from("<I", raw, p)[0]
    pe = u32(0x3C)
    assert raw[pe : pe + 4] == b"PE\0\0"
    machine, count = u16(pe + 4), u16(pe + 6)
    optional = pe + 24
    magic = u16(optional)
    assert magic in (0x10B, 0x20B)
    directory = optional + (96 if magic == 0x10B else 112)
    sections = []
    for n in range(count):
        p = optional + u16(pe + 20) + n * 40
        sections.append(
            {
                "name": raw[p : p + 8].rstrip(b"\0").decode("ascii"),
                "size": u32(p + 8),
                "rva": u32(p + 12),
                "raw-size": u32(p + 16),
                "offset": u32(p + 20),
            }
        )

    def offset(rva, size):
        for row in sections:
            if row["rva"] <= rva and rva + size <= row["rva"] + row["raw-size"]:
                p = row["offset"] + rva - row["rva"]
                assert p + size <= len(raw)
                return p
        raise ValueError(("unbacked RVA", hex(rva), size))

    def word(rva):
        return u32(offset(rva, 4))

    def name(rva):
        p = offset(rva, 1)
        end = raw.find(b"\0", p, min(p + 1024, len(raw)))
        assert end >= p
        return raw[p:end].decode("ascii")

    rva, size = u32(directory), u32(directory + 4)
    row = {
        "path": str(path),
        "bytes": len(raw),
        "sha256": hashlib.sha256(raw).hexdigest(),
        "machine": machine,
        "image-base": struct.unpack_from(
            "<I" if magic == 0x10B else "<Q",
            raw,
            optional + (28 if magic == 0x10B else 24),
        )[0],
        "sections": sections,
        "exports": [],
    }
    if not rva:
        return row
    p = offset(rva, 40)
    functions, names = u32(p + 20), u32(p + 24)
    assert functions <= 65536 and names <= 65536
    ft, nt, ot = u32(p + 28), u32(p + 32), u32(p + 36)
    for i in range(names):
        symbol = name(word(nt + i * 4))
        if any(
            part in symbol.lower()
            for part in ["process", "threadsetup", "gdiinit", "clientpfn"]
        ):
            row.setdefault("related-export-names", []).append(
                {"name": symbol, "rva": word(ft + u16(offset(ot + i * 2, 2)) * 4)}
            )
        if symbol not in (
            "NtGdiInit2",
            "NtGdiInit",
            "NtGdiInitSpool",
            "W32pServiceTable",
            "W32pServiceLimit",
            "Wow64Transition",
        ):
            continue
        index = u16(offset(ot + i * 2, 2))
        assert index < functions
        target = word(ft + index * 4)
        item = {"name": symbol, "rva": target}
        if rva <= target < rva + size:
            item["forwarder"] = name(target)
        else:
            at = offset(target, 512)
            item["code-512"] = raw[at : at + 512].hex()
        row["exports"].append(item)
    return row


try:
    for file in [
        "win32k.sys",
        "win32kbase.sys",
        "win32kfull.sys",
        "win32u.dll",
        "gdi32full.dll",
        "../SysWOW64/win32u.dll",
    ]:
        p = Path("C:/Windows/System32") / file
        before = image(p)
        after = hashlib.sha256(p.read_bytes()).hexdigest()
        assert before["sha256"] == after
        result["images"].append(before)
    result.update(state="finished", **{"exit-code": 0})
except BaseException:
    result.update(state="failed", **{"exit-code": 1, "error": traceback.format_exc()})
OUT.write_text(json.dumps(result, indent=2) + "\n")
print(
    json.dumps(
        {
            "state": result["state"],
            "exit-code": result["exit-code"],
            "images": len(result["images"]),
        }
    ),
    flush=True,
)
raise SystemExit(result["exit-code"])
