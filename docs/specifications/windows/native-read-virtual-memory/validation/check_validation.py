"""Verify host receipts for the frozen input proof without self-reference."""

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
    for row in manifest["files"]:
        path = Path(row["file"])
        require(not path.is_absolute() and ".." not in path.parts, "receipt path")
        require(identity(here / path) == pair(row), "receipt identity " + str(path))
    expected = {row["file"] for row in manifest["files"]} | {"manifest.json"}
    require(
        expected == {p.name for p in here.iterdir() if p.is_file()},
        "complete receipt inventory",
    )
    for host, name in [
        ("macos", "read-memory-evidence-macos-final-v2.json"),
        ("linux", "read-memory-evidence-linux-final-v2.json"),
        ("windows", "read-memory-evidence-windows-v2.json"),
    ]:
        receipt = json.loads((here / name).read_bytes())
        require(
            receipt["state"] == "finished" and receipt["exit-code"] == 0,
            host + " verifier exit",
        )
        for field, file in [("proof", "proof.json"), ("checker", "check_evidence.py")]:
            require(
                receipt[field + "-before"] == receipt[field + "-after"],
                host + " stable " + field,
            )
            require(
                pair(receipt[field + "-before"])
                == pair(manifest[field])
                == identity(source / file),
                host + " exact " + field,
            )
        row = receipt["log"]
        name = row.get("file", row.get("path")).replace("\\", "/").split("/")[-1]
        require(identity(here / name) == pair(row), host + " verifier log")
        result = json.loads((here / name).read_bytes())
        require(
            result["native-calls"] == 21140
            and result["rtl-calls"] == 28
            and result["compiler-inputs"] == 29173
            and result["owning-consumer-hosts"] == 3,
            "complete verifier result",
        )
        require(
            result["gates"]["windows"]["full-failed"] == 5
            and result["independent-x64-kernel"] == "unknown",
            "explicit result limits",
        )
        if host == "windows":
            require(
                receipt["os"] == "nt" and receipt["machine"] == "ARM64",
                "native Windows host",
            )
    negative = json.loads(
        (
            source / "evidence/read-memory-evidence-negative-controls-v1.json"
        ).read_bytes()
    )
    require(
        negative["state"] == "finished"
        and negative["exit-code"] == 0
        and negative["temporary-copy-removed"],
        "negative controls complete",
    )
    require(
        negative["checker-sha256"] == manifest["checker"]["sha256"],
        "negative checker binding",
    )
    require(
        len(negative["cases"]) == 13
        and sum(c["expected-rejection"] is not None for c in negative["cases"]) == 11,
        "negative selection",
    )
    for row in negative["cases"]:
        require(identity(source / "evidence" / row["log"]) == pair(row), "negative log")
        require(
            (row["exit-code"] != 0) == (row["expected-rejection"] is not None),
            "negative rejection",
        )
    print("Three host receipts and thirteen evidence-control outcomes verified")


if __name__ == "__main__":
    main()
