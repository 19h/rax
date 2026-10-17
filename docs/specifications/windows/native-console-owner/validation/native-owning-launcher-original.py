from pathlib import Path
import os,json,datetime,subprocess,sys
share=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');task=Path('C:/Windows/Temp/assist-native-rax-20261010');lock=task/'console-owner-owning-epoch2-driver.lock'
if (share/'console-owner-owning-epoch2-atomic-v2-launch.json').exists():raise RuntimeError('existinglaunchrecord;inspectbeforeanyretry')
try:fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY)
except FileExistsError:
 print('Owning driver already protected by task lock',lock.read_text(),flush=True);sys.exit(3)
os.write(fd,str(os.getpid()).encode());os.close(fd)
record=share/'console-owner-owning-epoch2-atomic-v2-launch.json';row={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'exclusive-lock':str(lock)}
try:
 with (share/'console-owner-owning-epoch2-atomic-v2-driver.log').open('wb') as out:
  args=['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(share/'assist-native-console-owner-epoch2-owned.ps1')];child=subprocess.Popen(args,stdout=out,stderr=subprocess.STDOUT);row['child-pid']=child.pid;row['argv']=args;record.write_text(json.dumps(row,indent=2)+'\n');row['exit-code']=child.wait()
finally:
 row['finished-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();record.write_text(json.dumps(row,indent=2)+'\n');lock.unlink()
print(json.dumps(row,indent=2),flush=True);sys.exit(row['exit-code'])
