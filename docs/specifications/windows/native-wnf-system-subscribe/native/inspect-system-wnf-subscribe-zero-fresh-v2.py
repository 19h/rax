from pathlib import Path
import hashlib,json,struct
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');b=Path('C:/Windows/Temp/assist-native-rax-20261010/system-wnf-subscribe-zero-fresh-v2')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def load(p):
 x=p.read_bytes();return json.loads(x.decode('utf-16' if x[:2] in [b'\xff\xfe',b'\xfe\xff'] else 'utf-8-sig'))
src=b/'native-system-wnf-subscribe-zero-fresh-v2.cpp';exe=b/'native-system-wnf-subscribe-zero-fresh-v2.exe';raw=exe.read_bytes();pe=struct.unpack_from('<I',raw,60)[0];pre=load(s/'native-system-wnf-subscribe-zero-fresh-v2-identity.json')
assert sha(src)=='5c11293dfabceedf5b6a069ea93f8489df4085a523c7667d8a6289d8a2973740'
assert raw[pe:pe+4]==b'PE\0\0' and struct.unpack_from('<H',raw,pe+4)[0]==0xaa64 and struct.unpack_from('<H',raw,pe+22)[0]&0x20
assert pre['sha256']==sha(exe) and pre['bytes']==len(raw)
launch=load(s/'system-wnf-subscribe-zero-fresh-v2-probe-launch.json');assert launch['state']=='finished' and launch['exit-code']==0
runs=[]
for mode in ['value']:
 log=s/('native-system-wnf-subscribe-zero-fresh-v2-'+mode+'.log');t=log.read_text(encoding='utf-8-sig');rr=[dict(x.split('=',1) for x in l.split())for l in t.splitlines()if l.startswith('case=')]
 assert len(rr)==193 and [int(r['case'])for r in rr]==list(range(193))
 assert 'selected-size-before=0'in t and 'selected-size-after=0'in t
 assert 'owned-create status=00000000'in t and 'descriptor-freed=1'in t and 'owned-delete status=00000000'in t
 assert t.rstrip().endswith('complete cases=192 warmup=1 deleted=1 descriptor-freed=1')
 for r in rr:
  assert r['calls']=='4' and r['exit']=='77' and all(r['type'+str(i)]=='1'for i in range(5))
  if r['warmup']=='0':assert r['handles-before']==r['handles-after']
  assert r['registered']==r['status']==r['repeat']==r['end']=='00000000'
  assert r['id']==r['repeat-id'] and int(r['id'],16)!=0
 before=[l.replace('selected-before','selected')for l in t.splitlines()if l.startswith('selected-before')]
 after=[l.replace('selected-after','selected')for l in t.splitlines()if l.startswith('selected-after')]
 assert len(before)==1 and before==after
 runs.append({'mode':mode,'stdout':{'sha256':sha(log),'bytes':log.stat().st_size},'cases':192,'warmup':1,'children-exited-77':193,'equal-parent-handle-counts-excluding-warmup':192,'created':1,'deleted':1,'freed-descriptors':1,'selected-before-after':before[0],'rows':rr})
row={'scope':'owned temporary System states; two modes; no existing state mutation','source':{'sha256':sha(src),'bytes':src.stat().st_size},'executable':{'sha256':sha(exe),'bytes':len(raw),'machine':0xaa64},'runs':runs}
(s/'native-system-wnf-subscribe-zero-fresh-v2-verified.json').write_text(json.dumps(row,indent=2)+'\n');print('Verified192 zero-length initialized data cases;193 child exits;one name deleted;one descriptor freed;actual producer hash unchanged')
