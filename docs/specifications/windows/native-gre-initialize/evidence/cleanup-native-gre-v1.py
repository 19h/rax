from pathlib import Path
import hashlib, json, subprocess, shutil, traceback

S = Path(r"C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
ROOT = Path(r"C:/Windows/Temp/assist-native-rax-20261010/src-display-candidate1")
R = ROOT / "target/native-gre-initialize-v1-four-views"
F = S / "native-gre-probe-cleanup-v1.json"
assert not F.exists()
state = {"state": "preparing", "path": str(R), "removed": False}


def save():
    F.write_text(json.dumps(state, indent=2) + "\n")


def identity(p):
    b = p.read_bytes()
    return len(b), hashlib.sha256(b).hexdigest()


save()
try:
    repeat = json.loads((S / "native-gre-repeat-v1.json").read_bytes())
    assert repeat["state"] == "finished" and repeat["exit-code"] == 0
    original = json.loads(
        (S / "native-gre-initialize-v1-four-views-launch.json").read_bytes()
    )
    assert (
        R.resolve().is_relative_to(ROOT.resolve()) and R.is_dir() and not R.is_symlink()
    )
    for row in [
        original["source-before"],
        *[o["executable-before"] for o in original["outcomes"]],
    ]:
        assert identity(Path(row["path"])) == (row["bytes"], row["sha256"])
    prefix = str(R).replace("'", "''")
    script = (
        "Get-CimInstance Win32_Process -ErrorAction Stop | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith('"
        + prefix
        + "',[System.StringComparison]::OrdinalIgnoreCase) } | Select-Object ProcessId,ExecutablePath | ConvertTo-Json -Compress"
    )
    query = subprocess.run(
        ["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", script],
        capture_output=True,
        timeout=30,
    )
    assert query.returncode == 0, query.stderr.decode(errors="replace")
    text = query.stdout.decode(encoding="utf-8-sig").strip()
    active = json.loads(text) if text else []
    assert active == [], active
    state["active-probe-processes"] = active
    shutil.rmtree(R)
    state.update(state="finished", removed=not R.exists(), **{"exit-code": 0})
except BaseException:
    state.update(state="failed", error=traceback.format_exc(), **{"exit-code": 1})
finally:
    save()
raise SystemExit(state["exit-code"])
