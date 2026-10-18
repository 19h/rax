from pathlib import Path
import datetime, json, os, subprocess, sys
b=Path('C:/Windows/Temp/assist-native-rax-20261010')
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record=s/'bno-final-post-run-completion.json'
if record.exists():raise RuntimeError('BNO completion already exists')
for name in ['bno-native-epoch5-launch.json','bno-final-owned-launch.json']:
 row=json.loads((s/name).read_text())
 if row['state']!='finished' or row['exit-code']!=0:raise RuntimeError('incomplete '+name)
lock=b/'bno-final-post-run.lock'
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(fd)
checks=[]
try:
 for script,log in [('run-bno-owner-observer.py','bno-owner-observer-transport.log'),('verify-bno-final-source.py','native-bno-final-source-post-run.log'),('verify-bno-post-run.py','native-bno-final-post-run-verifier.log')]:
  with (s/log).open('wb') as output:
   result=subprocess.run([sys.executable,'-B',str(s/script)],stdout=output,stderr=subprocess.STDOUT)
  checks.append({'script':script,'exit-code':result.returncode})
  if result.returncode:raise RuntimeError('BNO completion failed '+script)
 record.write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source-epoch':5,'namespace-epoch':2,'checks':checks},indent=2)+'\n')
finally:lock.unlink()
print('BNO post-run checks complete',flush=True)
