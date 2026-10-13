#!/usr/bin/env python3
"""Falsify provenance and independent partitions without changing the corpus."""
from contextlib import redirect_stdout
import argparse
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import shutil
import tempfile


def main(directory):
    original = Path(directory)
    spec = importlib.util.spec_from_file_location("evidence", original / "check-evidence.py")
    checker = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(checker)
    with redirect_stdout(io.StringIO()):
        checker.main(original)
    identity_name = "native-symbolic-link-open-producer-identities.json"
    inputs_name = "native-symbolic-link-open-producer-inputs.json"
    results = []
    with tempfile.TemporaryDirectory(prefix="rax-symbolic-link-evidence-") as temporary:
        root = Path(temporary) / "corpus"
        shutil.copytree(original, root, ignore=shutil.ignore_patterns("__pycache__"))
        originals = {}

        def write(name, data):
            path = root / name
            originals.setdefault(name, path.read_bytes())
            path.write_bytes(data)

        def store(name, data):
            write(name, (json.dumps(data, indent=2) + "\n").encode())

        def rehash_log(identity, producer, text):
            data = text.encode()
            write(producer["log"], data)
            producer["log_identity"]["bytes"] = len(data)
            producer["log_identity"]["sha256"] = hashlib.sha256(data).hexdigest()
            store(identity_name, identity)

        controls = [
            ("changed-compiled-source", "actual compiled source binding"),
            ("forged-pe-machine", "actual PE machine"),
            ("forged-x86-laa", "actual PE LAA"),
            ("missing-producer-and-input", "independent producer inventory"),
            ("changed-native-dll", "captured installed native DLL identities"),
            ("changed-rehashed-input", "independent input partition differs"),
            ("missing-rehashed-row", "independent input partition cardinality"),
            ("changed-rehashed-status", "native64 semantic mismatch"),
            ("changed-rehashed-fresh-lifetime", "fresh capture lifetime"),
            ("missing-rehashed-fresh-entry", "fresh original-entry/true-LR cardinality"),
            ("changed-rehashed-fresh-svc", "fresh live MEM_IMAGE SVC"),
        ]
        for name, expected_failure in controls:
            identity = json.loads((root / identity_name).read_text())
            producer = identity["producers"][0]
            if name == "changed-compiled-source":
                source = root / producer["source"]
                write(producer["source"], source.read_bytes() + b"\n// falsification\n")
            elif name == "forged-pe-machine":
                producer["executable_identity"]["machine"] = 0x14C
                store(identity_name, identity)
            elif name == "forged-x86-laa":
                producer = next(item for item in identity["producers"] if item["profile"] == "x86-laa")
                producer["executable_identity"]["characteristics"] &= ~0x20
                store(identity_name, identity)
            elif name == "missing-producer-and-input":
                inputs = json.loads((root / inputs_name).read_text())
                identity["producers"].pop(0)
                inputs.pop(0)
                store(identity_name, identity)
                store(inputs_name, inputs)
            elif name == "changed-native-dll":
                identity["native-libraries"][0]["sha256"] = "0" * 64
                store(identity_name, identity)
            else:
                if "fresh" in name:
                    producer = next(item for item in identity["producers"] if item["family"] == "fresh")
                text = (root / producer["log"]).read_text()
                lines = text.splitlines()
                index = next(i for i, line in enumerate(lines) if line.startswith("case=") and
                             (name != "changed-rehashed-fresh-lifetime" or "warmup=0" in line))
                if name == "missing-rehashed-fresh-entry":
                    lines = [line for line in lines if not line.startswith("entry ")]
                elif name == "changed-rehashed-fresh-svc":
                    position = next(i for i, line in enumerate(lines) if line.startswith("image "))
                    lines[position] = lines[position].replace("bytes=012700D4", "bytes=010000D4", 1)
                elif name == "missing-rehashed-row":
                    lines.pop(index)
                else:
                    field = {"changed-rehashed-input": "access",
                             "changed-rehashed-status": "status",
                             "changed-rehashed-fresh-lifetime": "exact-handle-set"}[name]
                    tokens = lines[index].split()
                    position = next(i for i, token in enumerate(tokens) if token.startswith(field + "="))
                    value = tokens[position].split("=", 1)[1]
                    replacement = "0" if field == "exact-handle-set" else format(int(value, 16) ^ 1, "08x")
                    tokens[position] = field + "=" + replacement
                    lines[index] = " ".join(tokens)
                rehash_log(identity, producer, "\n".join(lines) + "\n")
            try:
                with redirect_stdout(io.StringIO()):
                    checker.main(root)
            except ValueError as failure:
                if expected_failure not in str(failure):
                    raise ValueError(name + " rejected for an unrelated reason: " + str(failure)) from failure
                results.append({"control": name, "result": "rejected", "reason": str(failure)})
            else:
                raise ValueError("accepted falsified evidence: " + name)
            for filename, data in originals.items():
                (root / filename).write_bytes(data)
            originals.clear()
    print(json.dumps({"positive": "pass", "negative-controls": results}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", default=str(Path(__file__).parent))
    main(parser.parse_args().corpus)
