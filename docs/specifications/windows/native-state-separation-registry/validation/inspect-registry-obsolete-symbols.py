from pathlib import Path
import collections,json,shutil
B=Path('C:/Windows/Temp/assist-native-rax-20261010')
known=['target','target-bno-isolation-prototype','target-image-extension-final','target-io-completion-owned','target-io-completion-tests','target-section-map','target-system-time-final','target-wait-epoch3']
rows=[]
for name in known:
 root=B/name
 files=[{'path':str(p),'bytes':p.stat().st_size} for p in root.rglob('*.pdb') if p.is_file()]
 rows.append({'root':str(root),'files':files,'logical-bytes':sum(x['bytes'] for x in files)})
x={'disk':list(shutil.disk_usage(B)),'roots':rows}
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
(S/'registry-obsolete-symbols-inventory.json').write_text(json.dumps(x,indent=2)+'\n')
print(json.dumps({'disk':x['disk'],'roots':[{'root':r['root'],'files':len(r['files']),'bytes':r['logical-bytes']} for r in rows]}),flush=True)
