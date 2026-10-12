"""Reject controlled evidence and source corruptions for their intended reason."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys

if sys.flags.optimize:
    raise ValueError("optimized negative validation rejected")
base = Path(__file__).parent
source = base.resolve().parents[3]
cache = Path(sys.argv[1])
cache.mkdir(parents=True, exist_ok=True)
results = []


def manifest(directory):
    records = {str(p.relative_to(directory)): {"bytes": p.stat().st_size,
               "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
               for p in directory.rglob("*") if p.is_file() and p.name != "evidence-manifest.json"}
    (directory / "evidence-manifest.json").write_text(json.dumps(records, indent=2) + "\n")


cases = [
    ("changed-rehashed-native-status", "same-width native semantic agreement", "status"),
    ("missing-rehashed-native-case", "sequential cases", "case"),
    ("changed-rehashed-input-partition", "independent input partition", "partition"),
    ("forged-actual-pe-architecture", "actual PE architecture", "pe"),
    ("changed-current-compiled-source", "current compiled source", "source"),
    ("optimized-python", "optimized validation rejected", "optimized"),
]
for label, reason, kind in cases:
    directory = cache / label
    if directory.exists():
        raise ValueError("negative destination already exists: " + str(directory))
    shutil.copytree(base, directory)
    selected_source = source
    if kind in {"status", "case", "partition"}:
        name = "native-object-directory-open-matrix-arm64.log"
        lines = (directory / name).read_text().splitlines()
        if kind == "status":
            lines[0] = lines[0].replace("status=C0000022", "status=C000000D")
        elif kind == "case":
            lines.pop(0)
        else:
            lines[0] = lines[0].replace("access=00000000", "access=00000001")
        (directory / name).write_text("\n".join(lines) + "\n")
        identities = json.loads((directory / "native-object-directory-open-producer-identities.json").read_text(encoding="utf-8-sig"))
        row = next(r for r in identities["producers"] if r["log"] == name)
        row["log_identity"]["bytes"] = (directory / name).stat().st_size
        row["log_identity"]["sha256"] = hashlib.sha256((directory / name).read_bytes()).hexdigest()
        (directory / "native-object-directory-open-producer-identities.json").write_text(json.dumps(identities, indent=2))
    elif kind == "pe":
        path = directory / "native-object-directory-open-producer-identities.json"
        identities = json.loads(path.read_text(encoding="utf-8-sig"))
        identities["producers"][0]["executable_identity"]["machine"] = 0x8664
        path.write_text(json.dumps(identities, indent=2))
    elif kind == "source":
        selected_source = cache / "changed-compiled-source-root"
        selected_source.mkdir()
        for item in json.loads((base / "owning-source-hashes.json").read_text())["sources"]:
            target = selected_source / item["path"]
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source / item["path"], target)
        changed = selected_source / "src/user/windows/dll/native/directory.rs"
        changed.write_bytes(changed.read_bytes() + b"\n// controlled negative source change\n")
    manifest(directory)
    command = [sys.executable] + (["-O"] if kind == "optimized" else []) + [
        str(base / "check-evidence.py"), str(directory), str(selected_source)]
    run = subprocess.run(command, capture_output=True, text=True)
    (cache / (label + ".log")).write_text(run.stdout + run.stderr)
    if run.returncode == 0 or reason not in run.stderr:
        raise ValueError("negative did not reject for intended reason: " + label + "\n" + run.stderr)
    results.append({"case": label, "exit-code": run.returncode, "required-reason": reason,
                    "result": "rejected-for-intended-reason"})
print(json.dumps(results, indent=2))
