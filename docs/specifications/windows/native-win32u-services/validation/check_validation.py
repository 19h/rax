"""Bind completed host-verifier receipts to the frozen Win32u evidence proof."""

from pathlib import Path
import hashlib
import json


def require(value, message):
    if not value:
        raise ValueError(message)


def pair(row):
    return row["bytes"], row["sha256"]


def identity(path):
    raw = path.read_bytes()
    return len(raw), hashlib.sha256(raw).hexdigest()


def main():
    require(__debug__, "optimized Python is not a validation checker")
    here = Path(__file__).resolve().parent
    source = here.parent
    manifest = json.loads((here / "manifest.json").read_bytes())
    names = [row["file"] for row in manifest["files"]]
    require(len(names) == len(set(names)), "unique receipt inventory")
    for row in manifest["files"]:
        path = Path(row["file"])
        require(path.name == row["file"] and not path.is_absolute(), "receipt path")
        require(identity(here / path) == pair(row), "receipt identity " + str(path))
    require(
        set(names) | {"manifest.json"}
        == {p.name for p in here.iterdir() if p.is_file()},
        "complete receipt inventory",
    )
    for host in ["macos", "linux", "windows"]:
        receipt = json.loads((here / f"win32u-proof-{host}-v1.json").read_bytes())
        require(
            receipt["state"] == "finished"
            and receipt["exit-code"] == 0
            and receipt["host"] == host,
            host + " verifier exit",
        )
        require(
            receipt["inputs-before"] == receipt["inputs-after"],
            host + " stable verifier inputs",
        )
        for field, name in [("proof", "proof.json"), ("checker", "check_evidence.py")]:
            require(
                pair(receipt["inputs-before"][name])
                == pair(manifest[field])
                == identity(source / name),
                host + " exact " + field,
            )
        require(
            receipt["source-inputs"] == 29182
            and receipt["source-frame-sha256"]
            == "6140e8121129bbd12c871dac8525834b68aacff0b9484b027054ec0b58e45a62",
            host + " compiler frame",
        )
        log = receipt["log"]
        require(
            log["file"] == f"win32u-proof-{host}-v1.log"
            and identity(here / log["file"]) == pair(log),
            host + " verifier log",
        )
        require(
            (here / log["file"]).read_bytes().strip()
            == b"PASS: selected Win32u admission; explicit unsupported boundary; "
            b"native first-entry effects; three-host compiler and retained consumer evidence",
            host + " complete verifier result",
        )
    negative = json.loads(
        (source / "evidence/win32u-evidence-negative-controls-v1.json").read_bytes()
    )
    require(
        negative["state"] == "finished"
        and negative["exit-code"] == 0
        and negative["checker-sha256"] == manifest["checker"]["sha256"],
        "completed bound negative controls",
    )
    require(
        len(negative["outcomes"]) == 15
        and sum(row["expected-pass"] for row in negative["outcomes"]) == 2,
        "negative control selection",
    )
    for row in negative["outcomes"]:
        require(identity(source / "evidence" / row["log"]) == pair(row), "negative log")
        require((row["exit-code"] == 0) == row["expected-pass"], "negative outcome")
    print("Three host receipts and fifteen evidence-control outcomes verified")


if __name__ == "__main__":
    main()
