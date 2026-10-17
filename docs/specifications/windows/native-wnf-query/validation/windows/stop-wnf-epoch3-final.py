from pathlib import Path
import subprocess,json,datetime
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');r=json.loads((s/'wnf-query-final-retry-launch.json').read_text());pid=r['child-pid']
if r['state'] not in ['running-core','running-owning']:raise RuntimeError('native stage changed; inspect before stopping')
p=json.loads(subprocess.check_output(['powershell.exe','-NoProfile','-Command',"Get-CimInstance Win32_Process -Filter 'ProcessId="+str(pid)+"' | Select-Object ProcessId,ParentProcessId,CommandLine | ConvertTo-Json"],text=True))
if p['ParentProcessId']!=r['pid'] or ('native-wnf-query-final-gates.ps1'not in p['CommandLine']and'assist-native-wnf-query-final-owned.ps1'not in p['CommandLine']):raise RuntimeError('native task child identity mismatch')
a=['taskkill.exe','/PID',str(pid),'/T','/F'];done=subprocess.run(a,capture_output=True,text=True)
(s/'wnf-epoch3-final-stopped.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'reason':'native cross-scope falsification requires correcting uncaptured namespace admission','identity':p,'argv':a,'exit-code':done.returncode,'stdout':done.stdout,'stderr':done.stderr},indent=2)+'\n')
print(done.returncode,done.stdout,done.stderr)
