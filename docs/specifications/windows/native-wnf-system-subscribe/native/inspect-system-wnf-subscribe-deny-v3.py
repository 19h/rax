from pathlib import Path
import json,hashlib,struct
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');b=Path('C:/Windows/Temp/assist-native-rax-20261010/system-wnf-subscribe-deny-v3')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def load(p):
 r=p.read_bytes();return json.loads(r.decode('utf-16'if r[:2]in[b'\xff\xfe',b'\xfe\xff']else'utf-8-sig'))
src=b/'native-system-wnf-subscribe-deny-v3.cpp';exe=b/'native-system-wnf-subscribe-deny-v3.exe';log=s/'native-system-wnf-subscribe-deny-v3.log'
assert sha(src)=='db2a699d042704b136e4ee87e0cb3ddbc5d5653d25d33ae339e74a06791d41ce'
raw=exe.read_bytes();pe=struct.unpack_from('<I',raw,60)[0];assert raw[pe:pe+4]==b'PE\0\0'and struct.unpack_from('<H',raw,pe+4)[0]==0xaa64
pre=load(s/'native-system-wnf-subscribe-deny-v3-identity.json');assert pre['sha256']==sha(exe)and pre['bytes']==len(raw)
r=load(s/'system-wnf-subscribe-deny-v3-probe-launch.json');assert r['state']=='finished'and r['exit-code']==0
t=log.read_text(encoding='utf-8-sig');rows=[dict(x.split('=',1)for x in l.split()[1:])for l in t.splitlines()if l.startswith('case ')]
assert len(rows)==384 and [int(r['ordinal'])for r in rows]==list(range(384))
assert t.rstrip().endswith('complete cases=384 created=4 deleted=4 freed=4 subscriptions=144 unsubscribed=144 failed=0')
assert sum(l.startswith('delete index=')and l.endswith('status=00000000')for l in t.splitlines())==4
assert all(r['last-error']=='7E11ABCD'for r in rows)
record={'source-sha256':sha(src),'executable-sha256':sha(exe),'executable-bytes':len(raw),'machine':0xaa64,'stdout-sha256':sha(log),'stdout-bytes':log.stat().st_size,'calls':384,'deleted':4,'freed':4,'subscriptions':144,'unsubscribed':144}
(s/'native-system-wnf-subscribe-deny-v3-verified.json').write_text(json.dumps(record,indent=2)+'\n');print(record)
