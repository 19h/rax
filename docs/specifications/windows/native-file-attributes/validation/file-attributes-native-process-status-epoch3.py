import datetime,json,subprocess
from pathlib import Path
p=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
result=subprocess.run(['tasklist.exe','/fi','IMAGENAME eq rustc.exe','/fo','csv','/nh'],capture_output=True,text=True,timeout=30)
value={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'exit-code':result.returncode,'stdout':result.stdout,'stderr':result.stderr};(p/'file-attributes-native-process-status-epoch3.json').write_text(json.dumps(value,indent=2)+'\n');print(json.dumps(value),flush=True)
