from pathlib import Path
import datetime,hashlib,importlib.util,json,re,shutil
Q=Path(__file__).parent;D=Path('/Users/int/.assist-native-alpc-csr-20261021/docs/specifications/windows/native-alpc-csr');B=Q/'native-proof-stress-v1';assert not B.exists();B.mkdir();spec=importlib.util.spec_from_file_location('alpc_proof',D/'check_native_evidence.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
def digest(path):raw=path.read_bytes();return {'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
def rewrite(d,prefix,fn):
 m=json.loads((d/'native-producers.json').read_text());entry=next(x for x in m['accepted'] if x['prefix']==prefix);lp=d/'native'/entry['launch'];launch=json.loads(lp.read_text())
 for outcome in launch['outcomes']:
  path=d/'native'/outcome['log'];path.write_text(fn(path.read_text(),outcome['profile']))
  step=next(x for x in launch['steps'] if x['log']==path.name);step.update(digest(path))
 lp.write_text(json.dumps(launch,indent=2)+'\n');entry['launch-bytes']=lp.stat().st_size;entry['launch-sha256']=digest(lp)['sha256'];(d/'native-producers.json').write_text(json.dumps(m,indent=2)+'\n')
def case_change(text,case,key,value):
 lines=text.splitlines();prefix=f'CASE id={case} ';result=[]
 for line in lines:
  if line.startswith(prefix):line=re.sub(r'(?<![\w-])'+re.escape(key)+r'=[^ ]+',key+'='+value,line)
  result.append(line)
 return '\n'.join(result)+'\n'
def access(t,p):return case_change(t,53,'status','00000000')
def guards(t,p):return case_change(t,69,'send-after','00000004')
def wrap(t,p):
 for c in [48,49]:t=case_change(t,c,'status','C000002F')
 return t
def export(t,p):
 lines=t.splitlines()
 for i,line in enumerate(lines):
  if line.startswith('USER32 '):
   a,b=line.split('hex=',1);raw=bytearray.fromhex(b);raw[0]^=1;lines[i]=a+'hex='+raw.hex()
 return '\n'.join(lines)+'\n'
def reserved(t,p):
 lines=t.splitlines()
 for i,line in enumerate(lines):
  if line.startswith('MAP slot=16 ') and 'state=00002000' in line:
   span=int(re.search(r' bytes=(\d+)',line).group(1));line=line.replace('state=00002000','state=00001000').replace('protect=00000000','protect=00000002');line=line.replace('hex=','hex='+('00'*span)+' stable=1');lines[i]=line
 return '\n'.join(lines)+'\n'
def capture(t,p):
 lines=t.splitlines();selected=[i for i,l in enumerate(lines) if l.startswith('dump name=client-section-after-call ')];i=selected[-1];a,b=lines[i].split('hex=',1);raw=bytearray.fromhex(b);raw[0x758:0x760]=(1).to_bytes(8,'little');lines[i]=a+'hex='+raw.hex();return '\n'.join(lines)+'\n'
rows=[]
for name,prefix,fn in [('connect-right-bypassed','native-alpc-private-limits-v9',access),('both-guards-consumed','native-alpc-private-priority-v6',guards),('wow-word-wrap','native-alpc-private-limits-v9',wrap),('wrong-export-correlation','native-csr-client-connect-v7',export),('reserved-pages-readable','native-csr-client-connect-v6',reserved),('related-buffer-not-cleared','native-alpc-csr-trace-v1',capture)]:
 d=B/name;d.mkdir();shutil.copytree(D/'native',d/'native');shutil.copy2(D/'native-producers.json',d/'native-producers.json');positive=module.check(d);rewrite(d,prefix,fn)
 try:module.check(d)
 except ValueError as e:reason=str(e)
 else:raise AssertionError('semantic corruption accepted '+name)
 shutil.rmtree(d/'native');shutil.copytree(D/'native',d/'native');shutil.copy2(D/'native-producers.json',d/'native-producers.json');restored=module.check(d);rows.append({'case':name,'rejected':True,'reason':reason,'positive-before':positive,'fresh-restore':restored,'scope':'All execution-view logs modified consistently where applicable; log/launch/manifest digests repaired before negative check.'})
result={'state':'finished','utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'cases':rows};(Q/'alpc-native-proof-stress-v1.json').write_text(json.dumps(result,indent=2)+'\n');print([(r['case'],r['reason']) for r in rows])
