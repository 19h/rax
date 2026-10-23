"""Transfer immutable recovery inputs through prlctl stdin, without Home sharing."""

from pathlib import Path
import base64
import hashlib
import json
import subprocess
import zipfile

Q = Path(__file__).resolve().parent
S = Path("/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
D = Path("/Users/int/.assist-native-display-policy-20261022")
packet = D / "t/win32u-recovery-v2.zip"
assert packet.exists()
names = [
    "run-win32u-services-candidate2-windows.py",
    "run-win32u-services-native-owning-candidate2.py",
    "win32u-services-owned-cpp-build-candidate2.py",
    "capture-win32u-runtime-inputs-v2.py",
    "run-native-win32u-next-frontier-v3.py",
    "run-win32u-services-owning-capi-candidate3.py",
    "run-win32u-services-owning-candidate1-v1.py",
    "collect-win32u-services-archives-candidate1.py",
    "owning-linux-cpp-build-win32u-services-candidate1.py",
    "win32u-services-owning-source-hashes-candidate2.json",
    "native-win32u-next-frontier-v3.rs",
    "native-win32u-next-frontier-v3-source.json",
    "win32u-services-owning-capi.cpp",
    "win32u-services-owning-capi-source.json",
    "win32u-runtime-paths-v1.json",
    "win32u-services-candidate1-source.json",
    "win32u-services-candidate1-overrides.tar.gz",
    "current-cpp-consumers-wait-root.json",
    "mutant-native-compiler-env-epoch4.json",
    "mutant-native-baseline-failures.json",
    "native-test-alert-v1b-launch.json",
]
with zipfile.ZipFile(packet, "r") as archive:
    assert set(archive.namelist()) == set(names)
    for name in names:
        path = Q / name
        if not path.exists():
            path = S / name
        assert archive.read(name) == path.read_bytes(), name
raw = packet.read_bytes()
receiver = (Q / "receive-win32u-recovery-v3.py").read_bytes()
argument = "exec(bytes.fromhex(input()).decode())"
command = [
    "prlctl",
    "exec",
    "a5b14f9a-d292-4610-a04d-df4ca4337c9d",
    "C:\\Windows\\Temp\\assist-native-rax-20261010\\python\\python.exe",
    "-I",
    "-B",
    "-c",
    argument,
]
result = subprocess.run(
    command,
    input=receiver.hex() + "\n" + base64.b64encode(raw).decode() + "\n",
    capture_output=True,
    text=True,
    timeout=60,
)
assert result.returncode == 0, (result.returncode, result.stderr, result.stdout[:500])
receipt = json.loads(result.stdout)
assert receipt["state"] == "finished" and receipt["exit-code"] == 0
assert (
    receipt["packet-bytes"] == len(raw)
    and receipt["packet-sha256"] == hashlib.sha256(raw).hexdigest()
)
assert {row["file"] for row in receipt["files"]} == set(names)
for row in receipt["files"]:
    path = Q / row["file"]
    if not path.exists():
        path = S / row["file"]
    data = path.read_bytes()
    assert (len(data), hashlib.sha256(data).hexdigest()) == (
        row["bytes"],
        row["sha256"],
    )
(Q / "win32u-local-recovery-deployment-v2.json").write_text(
    json.dumps(receipt, indent=2) + "\n"
)
print(
    json.dumps(
        {
            "state": receipt["state"],
            "packet-bytes": len(raw),
            "verified-inputs": len(names),
            "children": receipt["children"],
        }
    ),
    flush=True,
)
