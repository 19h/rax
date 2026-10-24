"""Project bounded GUI scalars from private native logs; omit memory dumps."""

from pathlib import Path
import argparse
import hashlib
import json
import re
import struct


def require(condition, message):
    if not condition:
        raise ValueError(message)


def identity(raw):
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def fields(line):
    return dict(re.findall(r"(\S+?)=(\S*)", line))


def basename(path):
    return path.replace("\\", "/").rsplit("/", 1)[-1]


def verified(root, record):
    name = basename(record.get("file", record.get("path", "")))
    raw = (root / name).read_bytes()
    require(identity(raw) == {key: record[key] for key in ("bytes", "sha256")}, name)
    return name, raw


def project_log(raw):
    hooks = {}
    callbacks = []
    returns = []
    deltas = []
    phases = []
    values = []
    current = None
    finished = None
    for line in raw.decode("utf-8").splitlines():
        row = fields(line)
        if line.startswith("hook="):
            hooks[row["hook"]] = {key: int(row[key], 16) for key in ("address", "rva")}
        elif line.startswith("callback="):
            current = {
                "ordinal": int(row["callback"]),
                "index": int(row["index"]),
                "length": int(row["length"]),
            }
            callbacks.append(current)
            current_sp = int(row["sp"], 16)
            current_teb = int(row["teb"], 16)
        elif line.startswith("buffer=callback-frame "):
            data = bytes.fromhex(row["data"])
            require(current is not None and len(data) == 128, "callback frame extent")
            u64 = lambda offset: struct.unpack_from("<Q", data, offset)[0]
            u32 = lambda offset: struct.unpack_from("<I", data, offset)[0]
            hook = hooks["NtGdiInit2"]
            frame = {
                "input-pointer-relative-to-sp": u64(0) - current_sp,
                "length": u32(8),
                "index": u32(12),
                "interrupted-sp-relative-to-sp": u64(32) - current_sp,
                "resume-win32u-rva": u64(40) - (hook["address"] - hook["rva"]),
            }
            require(frame["input-pointer-relative-to-sp"] == 48, "input pointer offset")
            require(
                frame["interrupted-sp-relative-to-sp"] == 64, "interrupted stack offset"
            )
            require(frame["resume-win32u-rva"] == hook["rva"] + 4, "resume address")
            require(frame["length"] == current["length"] == 0, "input length")
            require(frame["index"] == current["index"] == 84, "callback index")
            current["frame"] = frame
        elif line.startswith("callback-return="):
            pointer = int(row["buffer"], 16)
            require(pointer in (0, 1), "unexpected callback output class")
            returns.append(
                {
                    "ordinal": int(row["callback-return"]),
                    "buffer-class": "null" if pointer == 0 else "invalid-one",
                    "length": int(row["length"]),
                    "status": int(row["status"], 16),
                }
            )
        elif line.startswith("phase="):
            phases.append(
                {
                    "phase": row["phase"],
                    "callback-table-present": int(row["callback-table"], 16) != 0,
                    "gdi-table-present": int(row["gdi-table"], 16) != 0,
                }
            )
        elif line.startswith(
            ("init2-return=", "nested-init2-return=", "repeat=", "worker-repeat=")
        ):
            text = row.get(
                "init2-return", row.get("nested-init2-return", row.get("result"))
            )
            value = int(text, 16)
            values.append(
                {
                    "kind": line.split("=", 1)[0],
                    "nonzero": value != 0,
                    "high32-nonzero": value >> 32 != 0,
                    "opaque-value-sha256": hashlib.sha256(
                        struct.pack("<Q", value)
                    ).hexdigest(),
                }
            )
        elif line.startswith("delta="):
            offset = int(row["offset"], 16)
            kind = row["delta"]
            before, after = int(row["before"], 16), int(row["after"], 16)
            if (
                kind == "callback-peb"
                and offset == 0x80
                or kind == "callback-teb"
                and offset in (0x800, 0x8D0, 0x910, 0x918)
            ):
                item = {
                    "record": kind,
                    "offset": offset,
                    "before-nonzero": before != 0,
                    "after-nonzero": after != 0,
                }
                if offset in (0x80, 0x800):
                    item.update(
                        {"before-bit3": bool(before & 8), "after-bit3": bool(after & 8)}
                    )
                elif offset == 0x910:
                    item["after"] = after
                elif offset == 0x918:
                    item["after-relative-to-teb"] = after - current_teb
                deltas.append(item)
        elif line.startswith("finished="):
            finished = {key: int(value) for key, value in row.items()}
    require(
        callbacks and all("frame" in item for item in callbacks),
        "no complete callback frames",
    )
    require(len(returns) == len(callbacks), "callback/return count")
    return {
        "callbacks": callbacks,
        "returns": returns,
        "phases": phases,
        "guest-body-deltas": deltas,
        "opaque-returns": values,
        "finished-marker": finished,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    root = args.input.resolve()
    require(not args.output.exists(), "output already exists")
    material = {}
    result = {
        "schema": 1,
        "scope": "ARM64 native callback controls and allocation geometry; selected scalar projection, no raw stack/TEB/desktop/GDI bytes or absolute host addresses.",
        "units": "All sizes and offsets are bytes; NTSTATUS fields are unsigned32 values.",
        "controls": [],
    }
    for version in (5, 6):
        name = f"native-win32k-callback-v{version}.json"
        receipt_raw = (root / name).read_bytes()
        receipt = json.loads(receipt_raw)
        require(receipt["state"] == "finished" and receipt["exit-code"] == 0, name)
        require(receipt["source-before"] == receipt["source-after"], "source changed")
        require(
            receipt["executable-before"] == receipt["executable-after"],
            "executable changed",
        )
        require(
            receipt["runtime-before"] == receipt["runtime-after"], "runtime changed"
        )
        source_name, source = verified(root, receipt["source-before"])
        material[source_name] = source
        material[name] = receipt_raw
        driver = f"run-native-win32k-callback-v{version}.py"
        material[driver] = (root / driver).read_bytes()
        for step in receipt["steps"]:
            log_name, raw = verified(root, step["log"])
            if step["name"] == "build":
                material[log_name] = raw
                continue
            outcome = next(
                item
                for item in receipt["native-outcomes"]
                if item["mode"] == step["name"]
            )
            require(outcome["exit-code"] == step["exit-code"], "outcome status")
            require(
                outcome["before"] == outcome["after"] == receipt["executable-before"],
                "outcome executable",
            )
            result["controls"].append(
                {
                    "controller-version": version,
                    "mode": step["name"],
                    "native-exit-code": step["exit-code"],
                    "original-private-log": {"name": log_name, **identity(raw)},
                    "projection": project_log(raw),
                }
            )
    name = "native-gui-views-v1.json"
    raw = (root / name).read_bytes()
    view = json.loads(raw)
    require(view["state"] == "finished" and view["exit-code"] == 0, "view control")
    projected = {"original": {"name": name, **identity(raw)}, "views": {}}
    for kind in ("gdi", "desktop"):
        item = view[kind]
        projected["views"][kind] = {
            "bytes": item["bytes"],
            "regions": [
                {
                    "offset": region["base"] - item["base"],
                    **{
                        key: value
                        for key, value in region.items()
                        if key not in ("base", "allocation")
                    },
                }
                for region in item["regions"]
            ],
        }
    client = {item["offset"]: item["value"] for item in view["client-fields"]}
    projected["client-scalars"] = {
        hex(offset): client[offset] for offset in (0x800, 0x810, 0x890, 0x898)
    }
    projected["client-desktop-relative-pointers"] = {
        hex(offset): client[offset] - view["desktop"]["base"]
        for offset in (0x820, 0x828, 0x860)
    }
    seed = bytes.fromhex(view["client-thread-first-32"])
    require(len(seed) == 32 and not any(seed[:20]) and not any(seed[24:]), "seed shape")
    projected["seed"] = {
        "observed-bytes": 32,
        "zero-prefix-bytes": 20,
        "clock-offset": 20,
        "clock-bytes": 4,
        "zero-suffix-bytes": 8,
        "ABI-extent": "unknown",
    }
    result["allocation-control"] = projected
    material["capture-native-gui-views-v1.py"] = (
        root / "capture-native-gui-views-v1.py"
    ).read_bytes()
    name = "native-callback-return-v1.json"
    raw = (root / name).read_bytes()
    no_callback = json.loads(raw)
    require(
        no_callback["state"] == "finished"
        and no_callback["cases-recorded"] == len(no_callback["cases"]) == 80,
        "no-callback cohort",
    )
    require(
        no_callback["handle-count-before"] == no_callback["handle-count-after"],
        "handle count",
    )
    require(
        no_callback["python-image-before"] == no_callback["python-image-after"]
        and no_callback["ntdll-before"] == no_callback["ntdll-after"],
        "no-callback inputs",
    )
    require(
        all(
            item["returned-status"] == 0xC0000258 and item["last-error"] == 0x12345678
            for item in no_callback["cases"]
        ),
        "no-callback results",
    )
    material[name] = raw
    material["capture-native-callback-return-v1.py"] = (
        root / "capture-native-callback-return-v1.py"
    ).read_bytes()
    material["observations.json"] = (json.dumps(result, indent=2) + "\n").encode()
    producer_name = Path(__file__).name
    material[producer_name] = Path(__file__).read_bytes()
    args.output.mkdir(parents=True)
    for name, raw in material.items():
        (args.output / name).write_bytes(raw)
    index = {
        "scope": result["scope"],
        "files": [{"path": name, **identity(raw)} for name, raw in material.items()],
    }
    (args.output / "inputs.json").write_text(json.dumps(index, indent=2) + "\n")
    print(
        json.dumps(
            {
                "state": "finished",
                "controls": len(result["controls"]),
                "files": len(material),
                "output": str(args.output),
            }
        )
    )


if __name__ == "__main__":
    main()
