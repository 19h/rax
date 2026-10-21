from pathlib import Path
import json,subprocess,datetime
S=Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
rows=[]
for stem in ["process-mitigation-native-owning-candidate2-retry1","process-mitigation-native-ordinary-candidate2-retry1","process-mitigation-owning-capi-candidate2-retry1"]:
 p=S/(stem+".json");x=json.loads(p.read_text())
 assert not x["steps"] and x["state"] in ["waiting-for-focus","waiting-for-owning-archives","preparing"],x["state"]
 pid=int(x["pid"]);script="run-"+stem+".py"
 command='$taskProcess = Get-CimInstance Win32_Process -Filter "ProcessId = '+str(pid)+'"; if (!$taskProcess) {exit 4}; if ($taskProcess.CommandLine -notlike "*'+script+'*") {exit 5}; $taskProcess.CommandLine; Stop-Process -Id '+str(pid)+' -Force'
 r=subprocess.run(["powershell.exe","-NoProfile","-Command",command],capture_output=True,text=True)
 assert r.returncode==0,(stem,r.returncode,r.stdout,r.stderr)
 rows.append({"receipt":p.name,"original":x,"pid":pid,"verified-command-line":r.stdout.strip(),"action":"cancelled idle task-owned waiter; replacement watches immutable retry receipt"})
(S/"process-mitigation-superseded-waiters-v2.json").write_text(json.dumps({"utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),"rows":rows},indent=2)+"\n")
print("cancelled three verified idle task-owned waiters",flush=True)
