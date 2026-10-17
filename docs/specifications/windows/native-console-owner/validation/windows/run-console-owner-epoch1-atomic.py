from pathlib import Path
import os,json,datetime,subprocess,sys
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');r=Path('C:/Windows/Temp/assist-native-rax-20261010');record=s/'console-owner-epoch1-launch.json';lock=r/'console-owner-epoch1-driver.lock'
if record.exists() and 'child-pid' in json.loads(record.read_text()):raise RuntimeError('CSR epoch3 was already launched;inspect recorded outcome before any new producer')
assert json.loads((s/'nls-initialize-epoch4-launch.json').read_text())['exit-code']==0
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.write(fd,str(os.getpid()).encode());os.close(fd)
row={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'lock':str(lock),'source-epoch':1}
try:
 with (s/'console-owner-epoch1-driver.log').open('wb') as out:
  args=['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(s/'native-console-owner-epoch1-gates.ps1')];child=subprocess.Popen(args,stdout=out,stderr=subprocess.STDOUT);row['child-pid']=child.pid;row['argv']=args;record.write_text(json.dumps(row,indent=2)+'\n');row['exit-code']=child.wait()
finally:
 row['finished-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();record.write_text(json.dumps(row,indent=2)+'\n');lock.unlink()
print(json.dumps(row,indent=2),flush=True);sys.exit(row['exit-code'])
