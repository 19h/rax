from pathlib import Path
import hashlib,json,struct
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');b=Path('C:/Windows/Temp/assist-native-rax-20261010')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def load(p):
 raw=p.read_bytes();return json.loads(raw.decode('utf-16'if raw[:2]in[b'\xff\xfe',b'\xfe\xff']else'utf-8-sig'))
rows=[]
for profile,machine in [('arm64',0xaa64),('x64',0x8664),('x86',0x14c),('x86-laa',0x14c)]:
 root=b/('system-wnf-subscribe-'+profile);stem='native-system-wnf-subscribe-v3-'+profile;src=root/'native-system-wnf-subscribe-v3.cpp';exe=root/(stem+'.exe');log=s/(stem+'.log');pre=load(s/(stem+'-identity.json'))
 if sha(src)!='edd443e8b06fa244b65d5226309818767675b4af139cf44d1273a65aeb4d8c9a':raise RuntimeError('actual source')
 raw=exe.read_bytes();pe=struct.unpack_from('<I',raw,60)[0]
 if raw[pe:pe+4]!=b'PE\0\0'or struct.unpack_from('<H',raw,pe+4)[0]!=machine or bool(struct.unpack_from('<H',raw,pe+22)[0]&0x20)!=(profile!='x86'):raise RuntimeError('actual PE')
 if pre['sha256']!=sha(exe):raise RuntimeError('producer changed after recorded execution')
 text=log.read_text(encoding='utf-8-sig');rr=[dict(x.split('=',1)for x in l.split()[1:])for l in text.splitlines()if l.startswith('case ')]
 if len(rr)!=768 or [int(r['ordinal'])for r in rr]!=list(range(768)):raise RuntimeError('complete rows')
 if 'complete cases=768 created=8 deleted=8 freed=8 subscriptions=279 unsubscribed=279 failed=0'not in text:raise RuntimeError('cleanup')
 rows.append({'profile':profile,'source-sha256':sha(src),'executable':{'sha256':sha(exe),'bytes':len(raw),'machine':machine},'stdout':{'sha256':sha(log),'bytes':log.stat().st_size},'cases':768,'created':8,'deleted':8,'freed':8,'subscriptions':279,'unsubscribed':279})
(s/'native-system-wnf-subscribe-v3-identities.json').write_text(json.dumps({'scope':'four producer ABIs on one ARM64 Windows kernel; only own temporary System names','producers':rows},indent=2)+'\n');print('Verified3072 native calls and all32 owned state deletions')
