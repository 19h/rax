"""Reject rebound semantic corruptions without changing the frozen source graph."""

from pathlib import Path
import hashlib
import importlib.util
import json
import shutil
import tempfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]


def load(path):
    return json.loads(path.read_text())


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def seal(directory):
    envelope = load(directory / "validation-envelope.json")
    envelope["files"] = [
        {
            "path": p.relative_to(directory).as_posix(),
            "bytes": len(raw),
            "sha256": hashlib.sha256(raw).hexdigest(),
        }
        for p in sorted(directory.rglob("*"))
        if p.is_file() and p.name != "validation-envelope.json"
        for raw in [p.read_bytes()]
    ]
    write(directory / "validation-envelope.json", envelope)


def main():
    spec = importlib.util.spec_from_file_location(
        "file_attribute_proof", HERE / "check_validation.py"
    )
    checker = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(checker)
    original = checker.identity
    immutable = {}

    def identity(path):
        path = Path(path)
        if (
            path.is_relative_to(ROOT)
            and not path.is_relative_to(HERE)
            and not path.is_relative_to(ROOT / "target")
        ):
            if path not in immutable:
                immutable[path] = original(path)
            return immutable[path]
        return original(path)

    checker.identity = identity
    checker.audit(HERE, ROOT)
    results = []
    with tempfile.TemporaryDirectory(
        prefix="file-attributes-negative-", dir=ROOT / "target"
    ) as temp:
        trial = Path(temp) / "proof"
        shutil.copytree(HERE, trial)
        saved = {
            p.relative_to(trial): p.read_bytes()
            for p in trial.rglob("*")
            if p.is_file()
        }

        def change(name, mutate):
            path = trial / "validation" / name
            value = load(path)
            mutate(value)
            write(path, value)

        def native(version, mutate):
            name = f"native-file-attributes-v{version}-launch.json"
            value = load(trial / "validation" / name)
            row = next(x for x in value["steps"] if x["name"] == "arm64")
            path = trial / "validation" / row["log"]
            before = path.read_text()
            after = mutate(before)
            assert after != before
            raw = after.encode()
            path.write_bytes(raw)
            row.update(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest())
            write(trial / "validation" / name, value)

        def control(name, mutate):
            for relative, raw in saved.items():
                (trial / relative).write_bytes(raw)
            seal(trial)
            checker.audit(trial, ROOT)
            mutate()
            seal(trial)
            try:
                checker.audit(trial, ROOT)
            except AssertionError:
                results.append({"name": name, "rejected": True})
            else:
                raise AssertionError("accepted semantic corruption: " + name)

        control(
            "native-missing-file-falsely-succeeds",
            lambda: native(
                8,
                lambda s: s.replace("role=2 status=C0000034", "role=2 status=00000000"),
            ),
        )
        control(
            "native-handle-leak",
            lambda: native(
                10,
                lambda s: __import__("re").sub(
                    r"handle-count-after=(\d+)",
                    lambda m: "handle-count-after=" + str(int(m[1]) + 1),
                    s,
                ),
            ),
        )
        control(
            "pair-status-falsified",
            lambda: native(
                10,
                lambda s: s.replace(
                    "role=196 status=C000000D", "role=196 status=00000000"
                ),
            ),
        )
        control(
            "unaligned-red-deleted",
            lambda: change(
                "file-attributes-unaligned-red-epoch1.json",
                lambda v: v.update({"exit-code": 0}),
            ),
        )
        control(
            "pair-red-deleted",
            lambda: change(
                "file-attributes-pair-red-epoch2.json",
                lambda v: v.update({"exit-code": 0}),
            ),
        )
        control(
            "frozen-source-substituted",
            lambda: change(
                "file-attributes-source-epoch3.json",
                lambda v: v["files"][0].update({"sha256": "0" * 64}),
            ),
        )
        control(
            "new-windows-failure",
            lambda: change(
                "file-attributes-native-epoch3-gates.json",
                lambda v: v["exact-existing-failures"].append("new::failure"),
            ),
        )

        def features():
            for name in [
                "native-file-attributes-epoch3-pre-cpp-archives.json",
                "native-file-attributes-epoch3-final-owning-archive-identities.json",
            ]:
                change(name, lambda v: v.update({"core-features": ["jit"]}))

        control("coherent-core-feature-drift", features)

        def wrong_core():
            def mutate(v):
                for key in ["core-before", "core-after"]:
                    v[key]["sha256"] = "0" * 64

            change("file-attributes-native-ordinary-epoch3.json", mutate)

        control("coherent-observer-core-substitution", wrong_core)
        control(
            "capi-failure-as-exit",
            lambda: change(
                "file-attributes-owning-capi-epoch3.json",
                lambda v: v["outcomes"][1].update({"reason": 4}),
            ),
        )
        control(
            "baseline-source-substituted",
            lambda: (
                trial
                / "baseline-source/src/user/windows/process/sched/tests/services_tests/file_attributes_tests.rs"
            ).write_bytes(b"substituted\n"),
        )

        def quality(field):
            value = load(trial / "quality.json")
            value[field] = True
            write(trial / "quality.json", value)

        control(
            "false-whole-goal-completion",
            lambda: quality("full-process-objective-complete"),
        )
        control(
            "false-independent-x64-kernel",
            lambda: quality("independent-physical-x64-kernel"),
        )
    value = {
        "all-rejected": True,
        "positive-restorations-verified": len(results),
        "controls": results,
        "scope": "Relevant receipt/log and enclosing byte ledger rebound before rejection. Only immutable source identities are cached within this stress runner.",
    }
    print(json.dumps(value, indent=2))


if __name__ == "__main__":
    main()
