from pathlib import Path
import hashlib,json,re,struct
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');b=Path('C:/Windows/Temp/assist-native-rax-20261010/system-wnf-subscribe-transitions')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def load(p):
 x=p.read_bytes();return json.loads(x.decode('utf-16' if x[:2] in [b'\xff\xfe',b'\xfe\xff'] else 'utf-8-sig'))
src=b/'native-system-wnf-subscribe-transitions.cpp';exe=b/'native-system-wnf-subscribe-transitions.exe';log=s/'native-system-wnf-subscribe-transitions.log';raw=exe.read_bytes();pe=struct.unpack_from('<I',raw,60)[0];pre=load(s/'native-system-wnf-subscribe-transitions-identity.json')
assert sha(src)=='f624c61f9d6085ac6aaeb5f379c326fca826324efa59c0bcfd7b4b46fcf6f2d5'
assert raw[pe:pe+4]==b'PE\0\0' and struct.unpack_from('<H',raw,pe+4)[0]==0xaa64 and struct.unpack_from('<H',raw,pe+22)[0]&0x20
assert pre['sha256']==sha(exe) and pre['bytes']==len(raw)
launch=load(s/'system-wnf-subscribe-transitions-probe-launch.json');assert launch['state']=='finished' and launch['exit-code']==0
t=log.read_text(encoding='utf-8-sig');rr=[dict(x.split('=',1) for x in l.split()) for l in t.splitlines() if l.startswith('case=')]
assert len(rr)==289 and [int(r['case']) for r in rr]==list(range(289))
assert 'selected-before stamp=00000001 bytes=0100000000000000' in t and 'selected-after stamp=00000001 bytes=0100000000000000' in t
assert t.rstrip().endswith('complete cases=288 warmup=1 owned-event-reset=288')
for r in rr:
 assert r['calls']=='4' and r['exit']=='77' and all(r['type'+str(i)]=='1' for i in range(5))
 if r['warmup']=='0':assert r['handles-before']==r['handles-after']
 mask=int(r['mask'],16);denied=False
 assert mask in [0,1,16,17] and int(r['next-mask'],16) in [0,1,16,17]
 assert r['registered']=='00000000'
 assert r['status']==r['repeat']==('C0000022' if denied else '00000000')
 assert r['end']==('C0000034' if denied else '00000000')
 if not denied:assert r['id']==r['repeat-id'] and int(r['id'],16)!=0
row={'scope':'ARM64 fresh child process behavior;288 measured cases plus one warmup;selected existing state read-only', 'source':{'sha256':sha(src),'bytes':src.stat().st_size},'executable':{'sha256':sha(exe),'bytes':len(raw),'machine':0xaa64},'stdout':{'sha256':sha(log),'bytes':log.stat().st_size},'cases':288,'warmup':1,'children-exited-77':289,'equal-parent-handle-counts-excluding-warmup':288,'selected-before-after':{'stamp':1,'data':'0100000000000000'},'rows':rr}
(s/'native-system-wnf-subscribe-transitions-verified.json').write_text(json.dumps(row,indent=2)+'\n');print('Verified288 mask/stamp transition cases;289 children exited;selected state unchanged;actual producer hash unchanged')
