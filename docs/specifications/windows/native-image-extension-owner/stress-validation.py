"""Reject deliberately incorrect proof sets after rebinding altered input digests."""
from pathlib import Path
import hashlib,json,shutil,subprocess,sys,tempfile
D=Path(__file__).resolve().parent
P=D.parents[3]
def load(p):
 b=p.read_bytes();return json.loads(b.decode('utf-16')if b[:2]in[b'\xff\xfe',b'\xfe\xff']else b.decode('utf-8-sig'))
def put(p,j):p.write_text(json.dumps(j,indent=2)+'\n')
def rebind(copy):
 v=copy/'validation';j=load(v/'inputs.json')
 for row in j['files']:
  b=(copy/row['path']).read_bytes();row.update(bytes=len(b),sha256=hashlib.sha256(b).hexdigest())
 put(v/'inputs.json',j)
results=[]
for case,expected in [('missing-focus','native focus'),('wrong-core-features','native owned archives'),('wrong-source','final source'),('false-ordinary-success','ordinary exit accounting'),('missing-native-memory','native memory adapter'),('optimized','optimized validation is rejected')]:
 with tempfile.TemporaryDirectory(prefix='image-proof-')as t:
  copy=Path(t)/'proof';shutil.copytree(D,copy)
  v=copy/'validation'
  if case=='missing-focus':
   p=v/'native-image-extension-final-focus-focus.log';p.write_text(p.read_text().replace('20 passed;','0 passed;'))
  elif case=='wrong-core-features':
   p=v/'native-image-extension-final-owning-archive-hashes.json';j=load(p);j['core-features']=['smir-jit'];put(p,j)
  elif case=='wrong-source':
   for name in ['epoch5-frozen-source.json','epoch6-frozen-source.json']:
    p=v/name;j=load(p);j['files']['src/user/windows/dll/native.rs']='0'*64;put(p,j)
  elif case=='false-ordinary-success':
   p=v/'ordinary-final-analysis.json';j=load(p);j[0]['guest_exit']=0;put(p,j)
  elif case=='missing-native-memory':
   p=v/'native-image-extension-final-integration.log';p.write_text(p.read_text().replace('4 passed;','0 passed;'))
  rebind(copy)
  args=[sys.executable,*(['-O']if case=='optimized'else[]),'-B',str(copy/'check-validation.py'),'--source-root',str(P)]
  r=subprocess.run(args,capture_output=True,text=True)
  if r.returncode==0 or expected not in r.stdout+r.stderr:raise RuntimeError(case+': unexpected checker result '+r.stdout+r.stderr)
  results.append({'case':case,'exit-code':r.returncode,'rejected-for':expected})
print(json.dumps(results,indent=2))
