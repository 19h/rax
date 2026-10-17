from pathlib import Path
import os,json,datetime,subprocess,sys
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');r=Path('C:/Windows/Temp/assist-native-rax-20261010');record=s/'console-owner-epoch2-launch.json';lock=r/'console-owner-epoch2-driver.lock'
if record.exists():
 existing = json.loads(record.read_text())
 if existing.get('exit-code') == 0:
  print('Final native epoch2 driver already completed; inspect its actual gates', flush=True)
  sys.exit(0)
 raise RuntimeError('Final native epoch2 already requested;inspect recorded outcome before any new producer')
initial = json.loads((s/'console-owner-epoch1-launch.json').read_text())
handoff_path = s/'console-owner-native-handoff.json'
handoff = json.loads(handoff_path.read_text()) if handoff_path.exists() else {}
if initial.get('exit-code') != 101 and not (handoff.get('state') == 'running-final-epoch2' and handoff.get('final-source-epoch') == 2):
 raise RuntimeError('Neither initial terminal state nor native final-source handoff is ready')
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.write(fd,str(os.getpid()).encode());os.close(fd)
row={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'lock':str(lock),'source-epoch':2}
try:
 with (s/'console-owner-epoch2-driver.log').open('wb') as out:
  args=['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(s/'native-console-owner-epoch2-gates.ps1')];child=subprocess.Popen(args,stdout=out,stderr=subprocess.STDOUT);row['child-pid']=child.pid;row['argv']=args;record.write_text(json.dumps(row,indent=2)+'\n');row['exit-code']=child.wait()
finally:
 row['finished-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();record.write_text(json.dumps(row,indent=2)+'\n');lock.unlink()
print(json.dumps(row,indent=2),flush=True);sys.exit(row['exit-code'])
