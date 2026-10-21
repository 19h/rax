"""Adversarial record checks with an independent positive restoration each time."""

import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--records", type=Path, required=True)
    p.add_argument("--source", type=Path, required=True)
    p.add_argument("--checker", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    args = p.parse_args()
    spec = importlib.util.spec_from_file_location("file_open_check", args.checker)
    checker = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(checker)
    names = []
    for version in ["v7", "v10", "sharing-v2"]:
        prefix = "native-file-open-" + version
        names += [prefix + ".cpp", prefix + "-launch.json"]
        names += [prefix + "-" + arch + ".log" for arch in checker.PROFILES]
    cases = [
        "missing-profile",
        "duplicate-profile",
        "changed-runtime",
        "changed-executable",
        "changed-producer",
        "missing-role",
        "duplicate-role",
        "forged-status",
    ]
    results = []
    with tempfile.TemporaryDirectory(
        prefix="file-open-audit-", dir=args.output.parent
    ) as directory:
        root = Path(directory)
        for name in cases:
            for relative in names:
                shutil.copy2(args.records / relative, root / relative)
            receipt_path = root / "native-file-open-v7-launch.json"
            receipt = json.loads(receipt_path.read_text())
            if name == "missing-profile":
                receipt["outcomes"].pop()
            elif name == "duplicate-profile":
                receipt["outcomes"][-1] = copy.deepcopy(receipt["outcomes"][0])
            elif name == "changed-runtime":
                receipt["runtime-modules-after"][0]["sha256"] = "0" * 64
            elif name == "changed-executable":
                receipt["outcomes"][0]["executable-after"]["sha256"] = "0" * 64
            elif name == "changed-producer":
                path = root / "native-file-open-v7.cpp"
                path.write_bytes(path.read_bytes() + b"\n")
            else:
                path = root / "native-file-open-v7-x86.log"
                lines = path.read_text().splitlines()
                selected = next(
                    i
                    for i, line in enumerate(lines)
                    if line.startswith("case role=285 ")
                )
                if name == "missing-role":
                    del lines[selected]
                elif name == "duplicate-role":
                    lines[selected] = lines[selected].replace(
                        "role=285 ", "role=284 ", 1
                    )
                else:
                    lines[selected] = lines[selected].replace(
                        "status=C0000005", "status=00000000", 1
                    )
                path.write_text("\n".join(lines) + "\n")
                raw = path.read_bytes()
                step = next(r for r in receipt["steps"] if r["name"] == "x86")
                step.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
            receipt_path.write_text(json.dumps(receipt, indent=2) + "\n")
            rejected = False
            try:
                checker.oracles(root, args.source)
            except (ValueError, OSError, KeyError) as error:
                rejected = True
                reason = str(error)
            if not rejected:
                raise ValueError("corrupt evidence accepted: " + name)
            for relative in names:
                shutil.copy2(args.records / relative, root / relative)
            positive = checker.oracles(root, args.source)
            results.append(
                {
                    "case": name,
                    "rejected": True,
                    "reason": reason,
                    "positive-restoration": positive,
                }
            )
    output = {
        "scope": "Eight oracle-record corruptions and eight fresh positive restorations. No source/product mutation; full source integrity is audited separately.",
        "cases": results,
    }
    args.output.write_text(json.dumps(output, indent=2) + "\n")
    print(
        json.dumps(
            {"negative-rejections": len(results), "positive-restorations": len(results)}
        )
    )


if __name__ == "__main__":
    main()
