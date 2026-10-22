from pathlib import Path
import hashlib
import json
import shutil
import subprocess

B = Path("C:/Windows/Temp/assist-native-rax-20261010/src-display-candidate1")
S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
F = S / "native-read-probe-cleanup-v2.json"
assert not F.exists()
repeat = json.loads((S / "native-read-repeat-v2.json").read_bytes())
assert repeat["state"] == "finished" and repeat["exit-code"] == 0
expected = []
for version in range(5, 6):
    launch = json.loads(
        (
            S / f"native-read-virtual-memory-v{version}-four-views-launch.json"
        ).read_bytes()
    )
    directory = B / "target" / f"native-read-virtual-memory-v{version}-four-views"
    assert directory.resolve().is_relative_to(B.resolve()) and directory.is_dir()
    assert not directory.is_junction() and not directory.is_symlink()
    source = directory / f"native-read-virtual-memory-v{version}.cpp"
    raw = source.read_bytes()
    assert (len(raw), hashlib.sha256(raw).hexdigest()) == (
        launch["source-before"]["bytes"],
        launch["source-before"]["sha256"],
    )
    expected.append((directory, launch))
# Query only the exact four task-owned subtrees. Do not retain unrelated
# processes or their command lines in evidence.
clauses = [
    "$_.ExecutablePath -like '" + str(path).replace("'", "''") + "\\*'"
    for path, _ in expected
]
script = (
    "$p = @(Get-CimInstance Win32_Process | Where-Object { "
    + " -or ".join(clauses)
    + " } | Select-Object ProcessId,Name,ExecutablePath); ConvertTo-Json -InputObject $p -Compress"
)
result = subprocess.run(
    ["powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", script],
    capture_output=True,
    text=True,
    timeout=60,
)
assert result.returncode == 0, result.stderr
active = json.loads(result.stdout)
assert active == [], active
record = {"state": "running", "active-probe-processes": active, "directories": []}
for directory, launch in expected:
    identities = []
    for outcome in launch["outcomes"]:
        path = Path(outcome["executable-after"]["path"])
        raw = path.read_bytes()
        assert (len(raw), hashlib.sha256(raw).hexdigest()) == (
            outcome["executable-after"]["bytes"],
            outcome["executable-after"]["sha256"],
        )
        identities.append(outcome["executable-after"])
    files = [path for path in directory.rglob("*") if path.is_file()]
    row = {
        "path": str(directory.resolve()),
        "files": len(files),
        "bytes": sum(path.stat().st_size for path in files),
        "generated-ownership": "Original launch source and four final executable hashes match; no active executable in exact subtree.",
        "executables-before-deletion": identities,
    }
    shutil.rmtree(directory)
    row["removed"] = not directory.exists()
    assert row["removed"]
    record["directories"].append(row)
    F.write_text(json.dumps(record, indent=2) + "\n")
record["state"] = "finished"
F.write_text(json.dumps(record, indent=2) + "\n")
