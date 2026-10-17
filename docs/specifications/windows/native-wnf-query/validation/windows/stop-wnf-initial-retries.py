from pathlib import Path
import subprocess,json,datetime
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');r=json.loads((s/'wnf-query-initial-launch.json').read_text());pid=r['child-pid']
g=json.loads((s/'native-wnf-query-initial-gates.json').read_text(encoding='utf-8-sig'));g=g if isinstance(g,list) else [g]
if not any(x['gate']=='query' and x['exit-code']==101 for x in g):raise RuntimeError('no completed failing native compile')
raw=subprocess.check_output(['powershell.exe','-NoProfile','-Command',"Get-CimInstance Win32_Process -Filter 'ProcessId="+str(pid)+"' | Select-Object ProcessId,ParentProcessId,CommandLine | ConvertTo-Json"],text=True)
p=json.loads(raw)
if p['ParentProcessId']!=r['pid'] or 'native-wnf-query-initial-gates.ps1' not in p['CommandLine']:raise RuntimeError('task-owned child identity changed')
a=['taskkill.exe','/PID',str(pid),'/T','/F'];result=subprocess.run(a,capture_output=True,text=True)
(s/'wnf-query-initial-stop.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'reason':'stop redundant retries after completed initial compile failure; preserve frozen source and first result','identity':p,'argv':a,'exit-code':result.returncode,'stdout':result.stdout,'stderr':result.stderr},indent=2)+'\n')
print(result.returncode,result.stdout,result.stderr)
