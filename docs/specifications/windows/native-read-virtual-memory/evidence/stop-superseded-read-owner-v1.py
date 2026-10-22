from pathlib import Path
import json
import os
import signal
import subprocess

S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
F = S / "native-read-idle-owner3-supersession-v1.json"
assert not F.exists()
state = json.loads((S / "read-memory-native-owning-candidate3.json").read_bytes())
assert state["state"] == "waiting-for-focus" and state["steps"] == []
pid = state["pid"]
command = f"Get-CimInstance Win32_Process -Filter 'ProcessId={pid}' | Select-Object ProcessId,ExecutablePath,CommandLine | ConvertTo-Json -Compress"
result = subprocess.run(
    ["powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", command],
    capture_output=True,
    text=True,
    timeout=60,
)
assert result.returncode == 0, result.stderr
process = json.loads(result.stdout) if result.stdout.strip() else None
if process:
    assert process["ProcessId"] == pid
    assert (
        str(process["ExecutablePath"])
        .lower()
        .endswith("assist-native-rax-20261010\\python\\python.exe")
    )
    assert "run-read-memory-native-owning-candidate3.py" in process["CommandLine"]
    os.kill(pid, signal.SIGTERM)
F.write_text(
    json.dumps(
        {
            "state": "finished",
            "superseded": "candidate3 idle owner only; candidate4 owns final validation",
            "pid": pid,
            "verified-command": process,
            "prior-state": state,
            "terminated": process is not None,
        },
        indent=2,
    )
    + "\n"
)
