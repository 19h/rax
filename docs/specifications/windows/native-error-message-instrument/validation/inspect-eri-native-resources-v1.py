from pathlib import Path
import datetime,json,os,shutil,subprocess
B=Path("C:/Windows/Temp/assist-native-rax-20261010");S=Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
f=S/"eri-native-resources-v1.json";assert not f.exists()
p=subprocess.run(["powershell.exe","-NoProfile","-NonInteractive","-Command","Get-CimInstance Win32_Process | Where-Object { $_.Name -match '^(cargo|rustc|link|rax|cl|alpc).*' } | Select-Object ProcessId,Name,ExecutablePath,CommandLine | ConvertTo-Json -Depth 3"],capture_output=True,timeout=30)
r={"utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),"free-bytes":shutil.disk_usage(B).free,"process-inspection-exit":p.returncode,"processes":p.stdout.decode(errors="replace"),"paths":[]}
for part in ["src-alpc-candidate1/target/retained-owning-candidate1","src-alpc-candidate2/target/retained-owning-candidate2","src-eri-red2/target/normal-candidate2/debug"]:
 d=B/part
 if d.exists():
  for x in d.iterdir():
   if x.is_file() and ("retained-owning" in part or x.suffix.lower() in [".pdb",".rlib",".obj",".o",".lib"]):r["paths"].append({"path":str(x),"bytes":x.stat().st_size})
f.write_text(json.dumps(r,indent=2)+"\n");print(json.dumps(r))
