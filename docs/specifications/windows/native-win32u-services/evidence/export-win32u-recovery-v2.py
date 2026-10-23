"""Export task-local receipt/log snapshots through stdout, without host shares."""

from pathlib import Path
import base64
import io
import json
import zipfile

root = Path(
    "C:/Windows/Temp/assist-native-rax-20261010/src-display-candidate1/target/win32u-validation-recovery-v2"
)
prefixes = (
    "win32u-services-candidate2-",
    "win32u-services-native-owning-candidate2",
    "native-win32u-services-candidate1-",
    "native-win32u-next-frontier-v3",
    "win32u-services-owning-capi-candidate3",
    "win32u-runtime-inputs-v2",
    "native-win32u-admission-boundary-v1",
)
files = {
    p.name: p
    for p in (root / "transfer").iterdir()
    if p.is_file() and p.suffix in [".json", ".log"] and p.name.startswith(prefixes)
}
files.update({p.name: p for p in root.glob("*.bootstrap.log")})
files["recovery-deployment.json"] = root / "deployment.json"
files["retired-legacy-processes-v1.json"] = root / "retired-legacy-processes-v1.json"
output = io.BytesIO()
total = 0
with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
    for name, path in sorted(files.items()):
        if not path.exists():
            continue
        data = path.read_bytes()
        assert len(data) <= 32 << 20
        if name.endswith(".json"):
            try:
                json.loads(data)
            except ValueError:
                continue
        total += len(data)
        assert total <= 64 << 20
        archive.writestr(name, data)
print(base64.b64encode(output.getvalue()).decode(), flush=True)
