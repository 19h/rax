from pathlib import Path
import json
import os
import signal
import subprocess

S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
F = S / "native-read-full4-supersession-v1.json"
assert not F.exists()
state = json.loads((S / "read-memory-candidate4-windows.json").read_bytes())
assert state["state"] == "running" and state["current-step"] == "full"
pid = state["child-pid"]
selected = state["test-artifact"]["executable"]
command = f"Get-CimInstance Win32_Process -Filter 'ProcessId={pid}' | Select-Object ProcessId,ExecutablePath,CommandLine | ConvertTo-Json -Compress"
result = subprocess.run(
    ["powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", command],
    capture_output=True,
    text=True,
    timeout=60,
)
assert result.returncode == 0, result.stderr
process = json.loads(result.stdout)
assert process["ProcessId"] == pid
assert Path(process["ExecutablePath"]).resolve() == Path(selected).resolve()
assert "--nocapture --test-threads=1" in process["CommandLine"]
os.kill(pid, signal.SIGTERM)
F.write_text(
    json.dumps(
        {
            "state": "finished",
            "reason": "Candidate4 full run superseded by compiled source-alignment regression and complete sixteen-alignment native capture; candidate5 performs final full validation.",
            "pid": pid,
            "verified-task-process": process,
            "source-frame": "757461bef7bb17dd3499c1e5228d9b563715c7ea8bffbaa1e210b30607e4c513",
            "completed-steps-before-supersession": state["steps"],
            "termination": "SIGTERM on exact task unit-test image only",
        },
        indent=2,
    )
    + "\n"
)
