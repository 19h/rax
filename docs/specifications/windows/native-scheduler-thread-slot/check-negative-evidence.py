"""Controlled tampering with updated checksums must fail semantic checks."""
from pathlib import Path
import hashlib,json,re,shutil,subprocess,sys,tempfile
base=Path(__file__).parent
cases=[]
with tempfile.TemporaryDirectory(prefix='rax-scheduler-slot-negatives-')as tmp:
 work=Path(tmp)/'evidence';shutil.copytree(base,work)
 def attempt(name,reason,mutation):
  originals={p:p.read_bytes()for p in work.rglob('*')if p.is_file()}
  try:
   mutation()
   # Rehash corpus files and actual producer stdout metadata intentionally.
   meta=json.loads((work/'native-scheduler-slot-producer-identities.json').read_text())
   for p in meta['producers']:
    path=work/p['log'];p['stdout-identity'].update(bytes=path.stat().st_size,sha256=hashlib.sha256(path.read_bytes()).hexdigest())
   (work/'native-scheduler-slot-producer-identities.json').write_text(json.dumps(meta,indent=2)+'\n')
   m=json.loads((work/'evidence-manifest.json').read_text())
   for path in m['files']:
    p=work/path
    if p.exists():m['files'][path]={'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
   (work/'evidence-manifest.json').write_text(json.dumps(m,indent=2)+'\n')
   r=subprocess.run([sys.executable,'-B',str(work/'check-evidence.py'),str(work)],capture_output=True,text=True)
   assert r.returncode!=0 and reason in r.stderr,(name,reason,r.stdout,r.stderr)
   cases.append({'mutation':name,'expected-rejection':reason,'exit-code':r.returncode})
  finally:
   for p,b in originals.items():p.write_bytes(b)
 def row_change(key,value,index=0):
  p=work/'native-scheduler-thread-slot-probe-v2-arm64.log';ls=p.read_text().splitlines();at=[i for i,x in enumerate(ls)if x.startswith('case=')][index];ls[at]=re.sub(r'(?<![\w-])'+re.escape(key)+r'=[^ ]+',key+'='+value,ls[at],count=1);p.write_text('\n'.join(ls)+'\n')
 def json_change(name,change):
  p=work/name;b=p.read_bytes();j=json.loads(b.decode('utf-16'if b.startswith(b'\xff\xfe')else'utf-8-sig'));change(j);p.write_text(json.dumps(j,indent=2)+'\n')
 attempt('wrong kernel','native-kernel-build',lambda:json_change('native-scheduler-slot-producer-identities.json',lambda j:j['kernel'].__setitem__(2,29682)))
 attempt('missing producer','actual-producer-count',lambda:json_change('native-scheduler-slot-producer-identities.json',lambda j:j['producers'].pop()))
 attempt('wrong executable machine','native-executable-machine',lambda:json_change('native-scheduler-slot-producer-identities.json',lambda j:j['producers'][0]['executable-identity'].__setitem__('machine',0x8664)))
 attempt('wrong LAA','native-executable-laa',lambda:json_change('native-scheduler-slot-producer-identities.json',lambda j:j['producers'][2]['executable-identity'].__setitem__('characteristics',0x20)))
 attempt('changed source','actual-native-source-identity',lambda:(work/'native-scheduler-thread-slot-probe-v2.cpp').write_text('// changed\n'))
 attempt('producer failure','producer-exit',lambda:json_change('native-scheduler-thread-slot-probe-v2-outcomes.json',lambda j:j[0].__setitem__('exit-code',21)))
 for key,value,reason,index in [('family','1','replay-partition',0),('case','1','replay-partition',0),('padding-mode','1','replay-partition',0),('width','4','replay-width-exception',0),('exception','C0000005','replay-width-exception',0),('exact-handle-set','0','replay-handle-restoration',0),('status','C00000BB','replay-status-order',0),('after-slot','0','replay-published-binding',0),('before-slot','1','replay-before-binding',0),('input-slot','1','replay-input-slot',0),('bytes0','A5'*96,'replay-caller-bytes',0),('protection','00000004,00000104,','replay-guard-effect',0)]:
  attempt('changed '+key,reason,lambda k=key,v=value,i=index:row_change(k,v,i))
 def replace_file(name,old,new):
  p=work/name;x=p.read_text();assert old in x;p.write_text(x.replace(old,new,1))
 attempt('changed slot capacity','slot-capacity-status',lambda:replace_file('native-scheduler-thread-slot-capacity-arm64.log','status=C000009A','status=C00000BB'))
 def change_remote():
  p=work/'native-scheduler-thread-slot-boundaries-v2-arm64.log';ls=p.read_text().splitlines();at=next(i for i,line in enumerate(ls)if 'label=remote-matrix'in line);assert 'status=C00000BB'in ls[at];ls[at]=ls[at].replace('status=C00000BB','status=00000000',1);p.write_text('\n'.join(ls)+'\n')
 attempt('changed remote target result','remote-status-order',change_remote)
 attempt('changed release retention','mapping-reference-lifetime:active-close-retained',lambda:replace_file('native-scheduler-thread-slot-resources-v2-arm64.log','map label=active-close-retained','map label=other'))
 attempt('changed original SVC','fresh-original-svc-true-lr',lambda:replace_file('native-scheduler-thread-slot-fresh-v2-arm64.log','function=13','function=14'))
 attempt('changed exact true LR','fresh-original-svc-true-lr',lambda:replace_file('native-scheduler-thread-slot-fresh-v2-arm64.log','lr=7FF8DE3ADF78','lr=7FF8DE3ADF7C'))
 attempt('changed fresh birth','fresh-first-assignment-success',lambda:replace_file('native-scheduler-thread-slot-fresh-v2-arm64.log','status=00000000','status=C00000BB'))
 attempt('changed full page','fresh-full-page-zero',lambda:replace_file('native-scheduler-thread-slot-sequence-arm64.log','phase=original-slot-page address=','phase=modified-slot-page address='))
print(json.dumps({'positive-checker':'independent semantic checks','controlled-negatives':cases,'count':len(cases)},indent=2))
