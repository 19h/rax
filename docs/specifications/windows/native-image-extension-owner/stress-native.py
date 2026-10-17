from pathlib import Path
import gzip,hashlib,json,shutil,subprocess,sys,tempfile
D=Path(__file__).resolve().parent
CACHE=Path(sys.argv[1])
def write_json(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
def bind(d,path):
 m=json.loads((d/'native-inputs.json').read_text())
 for r in m['files']:
  if r['path']==path:
   raw=(d/path).read_bytes();r.update({'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
 write_json(d/'native-inputs.json',m)
def stdout_bind(d,profile):
 path='native/native-image-extension-owner-probe-v3-'+profile+'.log';bind(d,path)
 p=d/'native/image-extension-owner-v3-final-producer-identities.json';a=json.loads(p.read_text())
 for r in a:
  if r['profile']==profile:r['stdout-sha256']=hashlib.sha256((d/path).read_bytes()).hexdigest()
 write_json(p,a);bind(d,'native/'+p.name)
results=[]
for case in ['missing-case','wrong-protection-output','changed-machine','changed-handles','changed-rebase','optimized']:
 with tempfile.TemporaryDirectory(prefix='image-extension-negative-') as folder:
  d=Path(folder)/'evidence';shutil.copytree(D,d);cache=Path(folder)/'images';cache.mkdir()
  for row in json.loads((d/'external-images.json').read_text())['files']:
   shutil.copy2(CACHE/Path(row['path']).name,cache/Path(row['path']).name)
  if case in ['missing-case','wrong-protection-output']:
   p=d/'native/native-image-extension-owner-probe-v3-arm64.log';lines=p.read_text().splitlines()
   for i,line in enumerate(lines):
    if case=='missing-case' and line.startswith('case '):lines.pop(i);break
    if case=='wrong-protection-output' and line.startswith('protect '):lines[i]=line.replace('old=00000001','old=00000000');break
   p.write_text('\n'.join(lines)+'\n');stdout_bind(d,'arm64')
  elif case=='changed-machine':
   p=d/'native/image-extension-owner-v3-final-producer-identities.json';a=json.loads(p.read_text());a[0]['machine']=0x8664;write_json(p,a);bind(d,'native/'+p.name)
  elif case=='changed-handles':
   p=d/'native/native-image-extension-lifetime-v2.json';a=json.loads(p.read_text());a['handles-after']+=1;write_json(p,a);bind(d,'native/'+p.name)
  elif case=='changed-rebase':
   p=d/'native/native-image-extension-normalization.json';a=json.loads(p.read_text());row=next(r for r in a['rows']if r['index']==1)
   image=cache/row['archive'];raw=bytearray(gzip.decompress(image.read_bytes()));raw[1024]^=1;image.write_bytes(gzip.compress(raw,mtime=0));row['sha256']=hashlib.sha256(raw).hexdigest();row['archive-sha256']=hashlib.sha256(image.read_bytes()).hexdigest();write_json(p,a);bind(d,'native/'+p.name)
   m=json.loads((d/'external-images.json').read_text())
   for r in m['files']:
    if Path(r['path']).name==image.name:r.update({'bytes':image.stat().st_size,'sha256':hashlib.sha256(image.read_bytes()).hexdigest()})
   write_json(d/'external-images.json',m)
  args=[sys.executable,*(['-O']if case=='optimized'else[]),'-B',str(d/'check-native.py'),'--image-cache',str(cache)]
  run=subprocess.run(args,capture_output=True,text=True);expected={'missing-case':'ordered native case partition','wrong-protection-output':'native protection outputs','changed-machine':'actual PE machine','changed-handles':'handle count changed','changed-rebase':'independent native rebase closure','optimized':'optimized checker rejected'}[case]
  if run.returncode==0 or expected not in run.stderr:raise RuntimeError(case+' did not reject for '+expected+'\n'+run.stdout+run.stderr)
  results.append({'case':case,'rejected-for':expected,'exit-code':run.returncode})
print(json.dumps(results,indent=2))
