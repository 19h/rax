"""Wait for complete Linux gates, retain their original bytes, and validate."""

from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import time

Q = Path(__file__).resolve().parent
R = Path("/Users/int/.assist-native-display-policy-20261022")
D = R / "docs/specifications/windows/native-read-virtual-memory"
F = Q / "read-memory-linux-candidate5-recovery-v2-gates.json"
while True:
    gates = json.loads(F.read_bytes())
    if gates["state"] in ["finished", "failed"]:
        break
    time.sleep(3)
assert gates["state"] == "finished" and gates["exit-code"] == 0
assert [s["name"] for s in gates["steps"]] == [
    "ordinals",
    "focus",
    "full",
    "capi",
    "integration",
    "all-targets",
]
for name in [F.name, Path(__file__).name, *[s["log"] for s in gates["steps"]]]:
    source = Q / name
    destination = D / "evidence" / name
    assert not destination.exists() or destination.read_bytes() == source.read_bytes()
    shutil.copy2(source, destination)
subprocess.run([sys.executable, "-B", str(D / "build_proof.py")], cwd=R, check=True)
log = Q / "read-memory-evidence-macos-initial-v1.log"
assert not log.exists()
with log.open("wb") as stream:
    child = subprocess.run(
        [sys.executable, "-B", str(D / "check_evidence.py")],
        cwd=R,
        stdout=stream,
        stderr=subprocess.STDOUT,
    )
print("Initial verifier", child.returncode, log, flush=True)
assert child.returncode == 0, log.read_text()[-3000:]
subprocess.run(
    [sys.executable, "-B", str(Q / "run-read-evidence-negative-controls-v1.py")],
    cwd=R,
    check=True,
)
print("Evidence and negative controls complete", flush=True)
