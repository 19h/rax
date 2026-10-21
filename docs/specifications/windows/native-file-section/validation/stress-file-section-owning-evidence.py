"""Exercise supplemental proof boundaries with a fresh positive after every corruption."""

import argparse
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
    spec = importlib.util.spec_from_file_location("owning_evidence", args.checker)
    check = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(check)
    cases = [
        "missing-owning-gate",
        "changed-owning-source-set",
        "forged-selected-artifact",
        "incomplete-archive-walk",
        "changed-native-control",
        "failed-smoke-zero-exit-field",
        "wrong-retained-core",
        "different-red-test",
    ]

    def validate(root):
        bindings = check.projections(root)
        archives = check.owning(root, args.source, bindings)
        check.ordinary(root, bindings, archives)
        check.regression(root, args.source)

    outcomes = []
    with tempfile.TemporaryDirectory(
        prefix="file-section-owning-stress-", dir=args.output.parent
    ) as temp:
        root = Path(temp)
        for case in cases:
            for path in args.records.iterdir():
                if path.is_file():
                    shutil.copy2(path, root / path.name)
            name = "macos-file-section-candidate1-owning-gates.json"
            if case in [
                "changed-owning-source-set",
                "forged-selected-artifact",
                "incomplete-archive-walk",
            ]:
                name = "macos-file-section-candidate1-archives-after.json"
            elif case in ["changed-native-control", "failed-smoke-zero-exit-field"]:
                name = "file-section-owning-capi-candidate1.json"
            elif case == "wrong-retained-core":
                name = "file-section-retained-owning-candidate1.json"
            elif case == "different-red-test":
                name = "file-section-red-test-body-binding.json"
            path = root / name
            value = json.loads(path.read_text())
            if case == "missing-owning-gate":
                value["steps"].pop()
            elif case == "changed-owning-source-set":
                value["sources"].pop()
                before = root / "macos-file-section-candidate1-archives-before.json"
                same = json.loads(before.read_text())
                same["sources"] = value["sources"]
                before.write_text(json.dumps(same, indent=2) + "\n")
            elif case == "forged-selected-artifact":
                value["core-artifact"]["fresh"] = not value["core-artifact"]["fresh"]
            elif case == "incomplete-archive-walk":
                value["archives"][0]["members"] = 0
                before = root / "macos-file-section-candidate1-archives-before.json"
                same = json.loads(before.read_text())
                same["archives"] = value["archives"]
                before.write_text(json.dumps(same, indent=2) + "\n")
            elif case == "changed-native-control":
                value["native-after"][0]["stdout-sha256"] = "0" * 64
            elif case == "failed-smoke-zero-exit-field":
                value["outcomes"][0].update(
                    reason=5,
                    diagnostic="emulator failure; zero exit-code field remains",
                )
                log = root / "file-section-owning-capi-candidate1-ordinary.log"
                lines = log.read_text().splitlines()
                index = next(
                    i for i, line in enumerate(lines) if line.startswith("api-result ")
                )
                lines[index] = "api-result " + json.dumps(value["outcomes"][0])
                raw = ("\n".join(lines) + "\n").encode()
                log.write_bytes(raw)
                step = next(r for r in value["steps"] if r["name"] == "ordinary")
                step.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
            elif case == "wrong-retained-core":
                value["files"][0]["sha256"] = "0" * 64
            else:
                value["test-body-binding"]["sha256"] = "0" * 64
            path.write_text(json.dumps(value, indent=2) + "\n")
            try:
                validate(root)
            except (ValueError, OSError, KeyError) as error:
                reason = str(error)
            else:
                raise ValueError("corrupt record accepted: " + case)
            for path in args.records.iterdir():
                if path.is_file():
                    shutil.copy2(path, root / path.name)
            validate(root)
            outcomes.append(
                {
                    "case": case,
                    "rejected": True,
                    "reason": reason,
                    "fresh-positive-restoration": True,
                }
            )
    result = {
        "negative-rejections": len(outcomes),
        "positive-restorations": len(outcomes),
        "scope": "Supplemental owning/native-control/ordinary-result/retention/red-test boundaries. Each trial uses copied records, no identity cache and a complete fresh positive restoration. Native Windows also rehashes the retained owning artifacts.",
        "cases": outcomes,
    }
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
