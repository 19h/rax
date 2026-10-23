"""Receive native local validation snapshots through the existing exec channel."""

from pathlib import Path
import base64
import io
import json
import subprocess
import zipfile

Q = Path(__file__).resolve().parent
source = (Q / "export-win32u-recovery-v3.py").read_bytes()
result = subprocess.run(
    [
        "prlctl",
        "exec",
        "a5b14f9a-d292-4610-a04d-df4ca4337c9d",
        "C:\\Windows\\Temp\\assist-native-rax-20261010\\python\\python.exe",
        "-I",
        "-B",
        "-c",
        "exec(bytes.fromhex(input()).decode())",
    ],
    input=source.hex() + "\n",
    capture_output=True,
    text=True,
    timeout=30,
)
assert result.returncode == 0, result.stderr
blob = base64.b64decode(result.stdout.strip(), validate=True)
received = []
with zipfile.ZipFile(io.BytesIO(blob)) as archive:
    for row in archive.infolist():
        assert Path(row.filename).name == row.filename and row.file_size <= 32 << 20
        raw = archive.read(row)
        target = Q / row.filename
        if target.exists() and target.read_bytes() != raw and target.suffix == ".json":
            previous = json.loads(target.read_bytes())
            assert previous.get("state") not in ["finished", "failed"], row.filename
        target.write_bytes(raw)
        received.append(row.filename)
for name in [
    "win32u-services-candidate2-windows.json",
    "win32u-services-native-owning-candidate2.json",
    "native-win32u-next-frontier-v3.json",
    "win32u-services-owning-capi-candidate3.json",
    "win32u-runtime-inputs-v2.json",
    "native-win32u-admission-boundary-v2.json",
]:
    path = Q / name
    if path.exists():
        row = json.loads(path.read_text())
        print(
            name,
            row.get("state"),
            row.get("current-step"),
            row.get("error"),
            flush=True,
        )
        if "steps" in row:
            print(
                [
                    (x.get("name"), x.get("exit-code"), x.get("results"))
                    for x in row["steps"]
                ],
                flush=True,
            )
print("received snapshots", len(received), "compressed bytes", len(blob), flush=True)
