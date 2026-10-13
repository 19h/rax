"""Semantic tampering must fail even after corpus and stdout checksums refresh."""
from pathlib import Path
import hashlib,json,re,shutil,subprocess,sys,tempfile
if sys.flags.optimize:raise ValueError('optimized evidence checks rejected')
base=Path(__file__).parent;cases=[]
with tempfile.TemporaryDirectory(prefix='rax-thread-token-negatives-')as tmp:
 work=Path(tmp)/'evidence';shutil.copytree(base,work)
 def attempt(name,reason,mutation):
  originals={p:p.read_bytes()for p in work.rglob('*')if p.is_file()}
  try:
   mutation()
   meta=json.loads((work/'native-thread-token-producer-identities.json').read_text(encoding='utf-8-sig'))
   for producer in meta['producers']:
    f=work/producer['log'];producer['stdout-identity'].update(bytes=f.stat().st_size,sha256=hashlib.sha256(f.read_bytes()).hexdigest())
   (work/'native-thread-token-producer-identities.json').write_text(json.dumps(meta,indent=2)+'\n')
   files={str(f.relative_to(work)):{'bytes':f.stat().st_size,'sha256':hashlib.sha256(f.read_bytes()).hexdigest()}for f in work.rglob('*')if f.is_file()and f.name!='evidence-manifest.json'}
   (work/'evidence-manifest.json').write_text(json.dumps({'files':files},indent=2)+'\n')
   result=subprocess.run([sys.executable,'-B',str(work/'check-evidence.py'),str(work)],capture_output=True,text=True)
   assert result.returncode!=0 and reason in result.stderr,(name,reason,result.stdout,result.stderr)
   cases.append({'mutation':name,'expected-rejection':reason,'exit-code':result.returncode})
  finally:
   for f,b in originals.items():f.write_bytes(b)
 def change_json(name,mutation):
  f=work/name;b=f.read_bytes();value=json.loads(b.decode('utf-16'if b.startswith(b'\xff\xfe')else'utf-8-sig'));mutation(value);f.write_text(json.dumps(value,indent=2)+'\n')
 def row_change(name,key,value,index=0,predicate=lambda line:line.startswith('case=')):
  f=work/name;ls=f.read_text().splitlines();at=[i for i,line in enumerate(ls)if predicate(line)][index]
  ls[at],count=re.subn(r'(?<![\w-])'+re.escape(key)+r'=[^ ]*',key+'='+value,ls[at],count=1);assert count==1
  f.write_text('\n'.join(ls)+'\n')
 meta='native-thread-token-producer-identities.json'
 attempt('wrong kernel','native-kernel-build',lambda:change_json(meta,lambda j:j['kernel'].__setitem__(2,29682)))
 attempt('wrong principal','native-principal',lambda:change_json(meta,lambda j:j.__setitem__('principal','unknown')))
 attempt('missing actual producer','actual-producer-count',lambda:change_json(meta,lambda j:j['producers'].pop()))
 attempt('wrong executable machine','native-executable-machine',lambda:change_json(meta,lambda j:j['producers'][0]['executable-identity'].__setitem__('machine',0x8664)))
 attempt('wrong LAA bit','native-executable-laa',lambda:change_json(meta,lambda j:j['producers'][2]['executable-identity'].__setitem__('characteristics',0x20)))
 attempt('changed actual source','actual-native-source-identity',lambda:(work/'native-thread-token-open-probe-v5.cpp').write_text('// replaced\n'))
 attempt('producer failure','producer-exit',lambda:change_json('native-thread-token-open-probe-v5-outcomes.json',lambda j:j[0].__setitem__('exit-code',21)))
 mature='native-thread-token-open-probe-v5-arm64.log'
 for key,value,reason in [('case','1','replay-partition'),('family','1','replay-partition'),('as-self','2','replay-partition'),('width','4','replay-width-exception'),('exception','C0000005','replay-width-exception'),('exact-token-and-input-handles','0','token-and-input-restoration'),('births','1','absent-no-birth'),('status','00000000','absent-kernel-copy-status'),('protection','4,104,','absent-guard-effect'),('bytes0','00'*64,'absent-caller-bytes')]:
  attempt('changed mature '+key,reason,lambda k=key,v=value:row_change(mature,k,v))
 attempt('changed extended negative selector','zero-extended-selector',lambda:row_change(mature,'input-thread','FFFFFFFFFFFFFFFD',predicate=lambda l:l.startswith('case=')and' thread=19 'in l))
 attempt('erased native positive token birth','present-token-native-birth',lambda:row_change(mature,'births','0',predicate=lambda l:l.startswith('case=')and' state=2 'in l and' thread=0 'in l and' status=00000000 'in l))
 attempt('erased anonymous restriction','anonymous-native-control',lambda:row_change(mature,'status','00000000',predicate=lambda l:l.startswith('case=')and' state=1 'in l and' thread=0 'in l and' family=0 'in l))
 pseudo='native-thread-token-pseudo-arm64.log'
 attempt('changed actual RTL mapping','rtl-no-token-error',lambda:row_change(pseudo,'rtl-status-no-token-error','5',predicate=lambda l:l.startswith('width=')))
 attempt('changed signed pseudo width','pseudo-full-width-input',lambda:row_change(pseudo,'input-thread','FFFFFFFD'))
 attempt('made pseudo token successful','pseudo-invalid-handle',lambda:row_change(pseudo,'status','00000000'))
 attempt('changed pseudo caller bytes','pseudo-output-copy',lambda:row_change(pseudo,'output-after','0'))
 boundary='native-thread-token-boundary-x86.log'
 attempt('changed cross-page copy status','boundary-status',lambda:row_change(boundary,'status','C000007C'))
 attempt('consumed readonly guard','boundary-guard-priority',lambda:row_change(boundary,'protection','2',index=3))
 attempt('allowed partial cross-page store','boundary-no-partial-store',lambda:row_change(boundary,'bytes','00'*16))
 fresh='native-thread-token-open-fresh-arm64.log'
 attempt('changed original SVC','fresh-original-svc36-ret',lambda:row_change(fresh,'bytes','A10400D4C0035FD6',predicate=lambda l:l.startswith('image ')))
 attempt('changed startup access','fresh-original-arguments',lambda:row_change(fresh,'args','FFFFFFFFFFFFFFFE,20008,0,873B5BE400,',predicate=lambda l:l.startswith('entry ')))
 attempt('changed true LR','fresh-return-lr',lambda:row_change(fresh,'lr','7FF8DE3ACED0',predicate=lambda l:l.startswith('entry ')))
 attempt('made fresh return success','fresh-return-no-token',lambda:row_change(fresh,'status','00000000',predicate=lambda l:l.startswith('return ')))
 attempt('changed fresh output bytes','fresh-unchanged-output',lambda:row_change(fresh,'data','A5'*8,predicate=lambda l:'phase=output-after'in l))
 attempt('lost fresh parent handle','fresh-parent-handles',lambda:row_change(fresh,'exact-handle-set','0',predicate=lambda l:l.startswith('case=1 ')))
print(json.dumps({'positive-checker':'independent provenance and semantic checks','controlled-negatives':cases,'count':len(cases)},indent=2))
