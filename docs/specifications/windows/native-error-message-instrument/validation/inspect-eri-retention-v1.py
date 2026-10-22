from pathlib import Path
import json,subprocess,shutil,datetime
B=Path("C:/Windows/Temp/assist-native-rax-20261010");S=Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154");T=B/"src-eri-candidate1/target/retained-owning-candidate1"
p=subprocess.run(["powershell.exe","-NoProfile","-NonInteractive","-Command","Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'python.exe' -and $_.CommandLine -like '*retain-eri-artifacts-candidate1.py*' } | Select-Object ProcessId,ExecutablePath,CommandLine | ConvertTo-Json -Depth 3"],capture_output=True,timeout=30)
r={"utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),"target-exists":T.exists(),"receipt-exists":(S/"eri-retained-owning-candidate1.json").exists(),"free-bytes":shutil.disk_usage(B).free,"process-inspection-exit":p.returncode,"retention-processes":p.stdout.decode(errors="replace"),"target-files":[{"path":str(f),"bytes":f.stat().st_size} for f in T.iterdir() if f.is_file()] if T.exists() else []}
f=S/"eri-retention-inspection-v1.json";assert not f.exists();f.write_text(json.dumps(r,indent=2)+"\n");print(json.dumps(r))
