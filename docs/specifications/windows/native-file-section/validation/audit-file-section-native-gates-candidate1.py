"""Audit completed required gates separately from an extra artifact-selector error."""

from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import re

B = Path("C:/Windows/Temp/assist-native-rax-20261010")
S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
R = B / "src-file-section-candidate1"
F = S / "file-section-candidate1-final-gates.json"
assert not F.exists()


def identity(path):
    h = hashlib.sha256()
    n = 0
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            h.update(block)
            n += len(block)
    return {"path": str(path), "bytes": n, "sha256": h.hexdigest()}


def pair(row):
    return row["bytes"], row["sha256"]


raw_path = S / "file-section-candidate1-windows.json"
raw = json.loads(raw_path.read_text())
assert raw["state"] == "failed" and raw["current-step"] == "core-build"
assert 'not row["fresh"]' in raw["error"]
names = [
    "toolchain",
    "clean-core",
    "test-build",
    "file-focus",
    "section-focus",
    "full",
    "capi",
    "integration",
    "all-targets",
    "core-build",
]
assert [row["name"] for row in raw["steps"]] == names
for row in raw["steps"]:
    assert row["exit-code"] == (101 if row["name"] == "full" else 0)
    assert pair(identity(S / row["log"])) == pair(row)
full = raw["steps"][5]
assert any(
    "7598 passed; 5 failed; 2 ignored; 0 measured; 0 filtered out;" in x
    for x in full["results"]
)
content = (S / full["log"]).read_text()
failures = set(
    re.findall(r"^    (\S+)\r?$", content[content.rfind("\nfailures:\n") :], re.M)
)
expected = set(
    json.loads((S / "mutant-native-baseline-failures.json").read_text())["failures"]
)
assert failures == expected == set(raw["exact-existing-failures"])
assert raw["test-image-before"] == raw["test-image-after"]
before = identity(Path(raw["test-image-before"]["path"]))
assert before == raw["test-image-before"]
records = []
for line in (S / raw["steps"][-1]["log"]).read_text().splitlines():
    try:
        record = json.loads(line)
    except ValueError:
        continue
    if (
        record.get("reason") == "compiler-artifact"
        and record["target"]["name"] == "rax"
        and not record["profile"]["test"]
    ):
        records.append(record)
assert len(records) == 1
core = records[0]
assert (
    core["fresh"]
    and core["features"] == []
    and Path(core["manifest_path"]).parent.resolve() == R.resolve()
)
source_path = S / "file-section-candidate1-source.json"
assert (
    identity(source_path)["sha256"]
    == "985b35da38a798311293a8c3c48599f107e7a358547cbf2e9499d8414291413a"
)
manifest = json.loads(source_path.read_text())


def check(row):
    expected = pair(row)
    if row["path"] == ".cargo/config.toml":
        assert expected == (
            552,
            "a198c84574684e95de13dbb1bdb7b6c918f90fdb490c649937f697fc9621f3cd",
        )
        expected = (
            374,
            "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
        )
    assert pair(identity(R / row["path"])) == expected, row["path"]


with ThreadPoolExecutor(max_workers=4) as pool:
    list(pool.map(check, manifest["files"]))
after = identity(Path(raw["test-image-before"]["path"]))
assert after == before
result = {
    "state": "verified",
    "source-receipt": identity(raw_path),
    "source-freeze": identity(source_path),
    "required-executed-steps": names[:9],
    "exact-existing-failures": sorted(expected),
    "live-test-image-before": before,
    "live-test-image-after": after,
    "source-inputs": len(manifest["files"]),
    "extra-core-artifact": core,
    "scope": "All nine required native build/execution gates completed with their declared results. The subsequent optional ordinary-core selector rejected fresh=true even though this same candidate core was already built during the C API gate. That harness error is retained, not recast as a successful driver. Actual ordinary execution is validated separately through the selected Assist owning core and carrier.",
}
F.write_text(json.dumps(result, indent=2) + "\n")
print(
    json.dumps(
        {
            "state": result["state"],
            "source-inputs": result["source-inputs"],
            "required-executed-steps": result["required-executed-steps"],
        }
    ),
    flush=True,
)
