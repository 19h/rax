from pathlib import Path
import json,datetime,subprocess,os
b=Path('C:/Windows/Temp/assist-native-rax-20261010');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
lock=b/'detached-process-parameters-native-driver.lock';record=s/'detached-process-parameters-native-launch.json'
if record.exists():raise RuntimeError('launch record exists; inspect before retry')
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.write(fd,str(os.getpid()).encode());os.close(fd)
r={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'lock':str(lock)}
try:
 with (s/'detached-process-parameters-native-driver.log').open('wb') as f:
  argv=['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(s/'native-detached-process-parameters-profiles.ps1')]
  p=subprocess.Popen(argv,stdout=f,stderr=subprocess.STDOUT);r.update({'child-pid':p.pid,'argv':argv});record.write_text(json.dumps(r,indent=2)+'\n');rc=p.wait();r['exit-code']=rc;r['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();record.write_text(json.dumps(r,indent=2)+'\n')
finally:lock.unlink()
raise SystemExit(rc)
