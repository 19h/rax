from pathlib import Path
import datetime
import hashlib
import json
import re
import subprocess

S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
R = Path("C:/Windows/Temp/assist-native-rax-20261010/src-display-candidate1")
F = S / "wnf-live-native-repeat-v2.json"
assert not F.exists()
text = (S / "wnf-live-native-candidate1-focus.log").read_text()
match = re.search(r"Running unittests src[/\\]lib\.rs \(([^\r\n]+)\)", text)
assert match
image = Path(match[1])
if not image.is_absolute():
    image = R / image
assert image.resolve().is_relative_to(R.resolve())


def identity(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            h.update(block)
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": h.hexdigest()}


before = identity(image)
name = "user::windows::wnf::query_tests::native_query_snapshot_captures_typed_state_and_initializes_private_process_state"
rows = []
for attempt in range(12):
    result = subprocess.run(
        [str(image), name, "--exact", "--nocapture", "--test-threads=1"],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert "ok. 1 passed; 0 failed; 0 ignored;" in result.stdout
    assert "wnf-native-system attempt=" in result.stdout
    rows.append(
        {
            "attempt": attempt,
            "exit-code": result.returncode,
            "stdout": result.stdout,
            "stderr": result.stderr,
        }
    )
assert identity(image) == before
parent = R / "target/wnf-live-parent-control-v1.exe"
parent_before = identity(parent)
expected = json.loads((S / "wnf-live-parent-native-v1.json").read_bytes())["image"]
assert (parent_before["bytes"], parent_before["sha256"]) == (
    expected["bytes"],
    expected["sha256"],
)
result = subprocess.run(
    [str(parent), name, "--exact", "--nocapture", "--test-threads=1"],
    capture_output=True,
    text=True,
)
assert result.returncode == 101 and "0 passed; 1 failed; 0 ignored;" in result.stdout
assert identity(parent) == parent_before
record = {
    "state": "finished",
    "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "candidate-before": before,
    "candidate-after": identity(image),
    "repeats": rows,
    "parent-before": parent_before,
    "parent-after": identity(parent),
    "parent-result": {
        "exit-code": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
    },
}
F.write_text(json.dumps(record, indent=2) + "\n")
print(
    json.dumps(
        {
            "state": "finished",
            "positive-runs": len(rows),
            "parent-exit-code": result.returncode,
            "image-sha256": before["sha256"],
        }
    )
)
