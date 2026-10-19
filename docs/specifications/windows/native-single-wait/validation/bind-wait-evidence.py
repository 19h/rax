from pathlib import Path
import datetime,hashlib,json
N=Path(__file__).resolve().parent;D=N/'wait-source/docs/specifications/windows/native-single-wait'
rows=[]
for p in D.rglob('*'):
 if not p.is_file()or p.name=='evidence.json'or'__pycache__'in p.parts:continue
 raw=p.read_bytes();rows.append({'path':p.relative_to(D).as_posix(),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
rows.sort(key=lambda row:row['path'])
(D/'evidence.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source-epoch':4,'files':rows},indent=2)+'\n')
print('Bound',len(rows),'evidence files')
