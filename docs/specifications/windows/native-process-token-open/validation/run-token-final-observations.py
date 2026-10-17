from pathlib import Path
import subprocess,json,hashlib,datetime,time
root=Path('C:/Windows/Temp/assist-native-rax-20261010');share=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');records=[]
programs=[root/'src-token-open/tests/fixtures/user/windows/bin/arm64/smoke.exe',root/'assist/native-process-probe.exe',Path('C:/Windows/System32/whoami.exe')]
for i,program in enumerate(programs):
 row={'path':str(program),'scope':'actual production startup via unchanged observer,native_libraries=true,slice_insns1,3Mturn ceiling;controller0 does not imply guest exit'}
 if program.exists():
  row['program-sha256']=hashlib.sha256(program.read_bytes()).hexdigest();log=root/('native-token-final-ordinary-'+str(i)+'.log');start=time.time()
  with log.open('wb') as f:
   try:row['controller-exit-code']=subprocess.run([str(root/'native-token-final-trace.exe'),str(program)],stdout=f,stderr=subprocess.STDOUT,timeout=900).returncode
   except subprocess.TimeoutExpired:row['controller-timeout-seconds']=900
  row['seconds']=time.time()-start;shutil_data=log.read_bytes();(share/log.name).write_bytes(shutil_data);row['log']=log.name;row['sha256']=hashlib.sha256(shutil_data).hexdigest()
 else:row['unavailable']='program file absent;not counted as execution'
 records.append(row);(share/'native-token-final-ordinary-outcomes.json').write_text(json.dumps(records,indent=2)+'\n');print('ordinary observation',i,'complete;guest terminal output requires review',flush=True)
