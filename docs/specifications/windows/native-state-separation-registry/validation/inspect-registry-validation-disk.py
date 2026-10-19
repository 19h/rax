from pathlib import Path
import json,shutil
B=Path('C:/Windows/Temp/assist-native-rax-20261010')
roots=[B/'target-wait-epoch3',B/'target-wait-owned']
rows=[]
for root in roots:
 files=[]
 for p in root.rglob('*'):
  if p.is_file():
   n=p.stat().st_size
   if n>=100*1024*1024:files.append({'path':str(p),'bytes':n,'modified':p.stat().st_mtime})
 rows.append({'root':str(root),'large-files':sorted(files,key=lambda x:x['bytes'],reverse=True)[:40]})
x={'disk':list(shutil.disk_usage(B)),'targets':rows}
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
(S/'registry-disk-inspection.json').write_text(json.dumps(x,indent=2)+'\n')
print(json.dumps(x),flush=True)
