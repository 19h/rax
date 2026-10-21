"""Reject semantic evidence corruption and restore a fresh positive each time."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


def fingerprint(path):
    raw = path.read_bytes()
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def mutate(root, case):
    version = 14 if case == "shared-coherence" else 2
    name = f"native-file-section-v{version}-launch.json"
    receipt_path = root / "validation" / name
    receipt = json.loads(receipt_path.read_text())
    changed_logs = []
    if case == "producer-source":
        path = root / "validation" / f"native-file-section-v{version}.cpp"
        path.write_bytes(path.read_bytes() + b"\n// corrupted producer\n")
        return
    if case == "runtime-after":
        receipt["runtime-modules-after"][0]["sha256"] = "0" * 64
    elif case == "executable-after":
        receipt["outcomes"][0]["executable-after"]["bytes"] += 1
    elif case == "pe-laa":
        for phase in ["before", "after"]:
            path = root / "validation" / f"native-file-section-v2-x86-{phase}.log"
            pe = json.loads(path.read_text())
            pe["large-address-aware"] = True
            write(path, pe)
            changed_logs.append(path)
    else:
        path = root / "validation" / f"native-file-section-v{version}-arm64.log"
        text = path.read_text()
        if case == "role-count":
            text = (
                "\n".join(
                    line
                    for line in text.splitlines()
                    if not line.startswith("case role=0 ")
                )
                + "\n"
            )
        elif case == "role-sequence":
            text = text.replace("case role=0 ", "case role=1 ", 1)
        elif case == "handle-balance":
            text, count = re.subn(
                r"handles-after=(\d+)",
                lambda m: "handles-after=" + str(int(m[1]) + 1),
                text,
            )
            assert count == 1
        elif case == "shared-coherence":
            lines = text.splitlines()
            matches = [
                i for i, line in enumerate(lines) if line.startswith("case role=1 ")
            ]
            assert len(matches) == 1
            lines[matches[0]] = lines[matches[0]].replace(
                "after=2468ACE0", "after=13579BDF"
            )
            text = "\n".join(lines) + "\n"
        else:
            raise ValueError(case)
        path.write_text(text)
        changed_logs.append(path)
    for path in changed_logs:
        for row in receipt["steps"]:
            if row["log"] == path.name:
                row.update(fingerprint(path))
    write(receipt_path, receipt)
    catalog_path = root / "native-producers.json"
    catalog = json.loads(catalog_path.read_text())
    row = next(r for r in catalog["accepted"] if r["version"] == version)
    row.update(fingerprint(receipt_path))
    write(catalog_path, catalog)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--records", type=Path, required=True)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--checker", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    assert not args.output.exists()
    args.work.mkdir(parents=True, exist_ok=True)
    cases = [
        ("producer-source", "native producer source"),
        ("runtime-after", "runtime-modules changed"),
        ("executable-after", "native executable changed"),
        ("role-count", "native role count"),
        ("role-sequence", "native role sequence"),
        ("handle-balance", "native resource balance"),
        ("pe-laa", "native PE LAA"),
        ("shared-coherence", "native shared image coherence"),
    ]
    results = []
    for case, reason in cases:
        with tempfile.TemporaryDirectory(
            prefix="file-section-proof-", dir=args.work
        ) as temporary:
            root = Path(temporary)
            shutil.copytree(args.records, root / "validation")
            shutil.copy2(
                args.records.parent / "native-producers.json",
                root / "native-producers.json",
            )
            mutate(root, case)
            command = [
                sys.executable,
                "-B",
                str(args.checker),
                "--source-root",
                str(args.source),
                "--records",
                str(root / "validation"),
                "--native-only",
            ]
            negative = subprocess.run(
                command, capture_output=True, text=True, timeout=900
            )
            assert negative.returncode != 0 and reason in negative.stderr, (
                case,
                negative.stdout,
                negative.stderr,
            )
            shutil.rmtree(root / "validation")
            shutil.copytree(args.records, root / "validation")
            shutil.copy2(
                args.records.parent / "native-producers.json",
                root / "native-producers.json",
            )
            positive = subprocess.run(
                command, capture_output=True, text=True, timeout=900
            )
            assert positive.returncode == 0, (case, positive.stdout, positive.stderr)
            result = {
                "case": case,
                "reason": reason,
                "rejected": True,
                "fresh-positive-restoration": True,
            }
            results.append(result)
            print(json.dumps(result), flush=True)
    write(
        args.output,
        {
            "state": "verified",
            "negative-rejections": len(results),
            "positive-restorations": len(results),
            "cases": results,
            "scope": "Native oracle semantics with self-consistent receipt/catalog rebinding; fresh complete positive native/source audit after every corruption. This does not substitute for executing product gates.",
        },
    )


if __name__ == "__main__":
    main()
