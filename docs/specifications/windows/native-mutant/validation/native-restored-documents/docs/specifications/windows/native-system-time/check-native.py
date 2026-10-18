from pathlib import Path
import collections,hashlib,json
D=Path(__file__).resolve().parent
N=D/'native'
def need(v,m):
 if not v:raise ValueError(m)
def load(p):
 raw=p.read_bytes();return json.loads(raw.decode('utf-16') if raw[:2]in [b'\xff\xfe',b'\xfe\xff']else raw.decode('utf-8-sig'))
def fields(s):return dict(p.split('=',1)for p in s.split()if '='in p)
need(__debug__,'optimized checker rejected')
seen=set()
for manifest in ['native-inputs.json','primary/sources.json']:
 for row in load(D/manifest)['files']:
  need(row['path'] not in seen,'duplicate input');seen.add(row['path']);raw=(D/row['path']).read_bytes()
  need(len(raw)==row['bytes'] and hashlib.sha256(raw).hexdigest()==row['sha256'],'input identity '+row['path'])
identities=load(N/'system-time-v2-final-producer-identities.json')
need({r['profile']for r in identities}=={'arm64','x64','x86','x86-laa'},'producer profiles')
source=hashlib.sha256((N/'native-system-time-v2-probe.cpp').read_bytes()).hexdigest()
for identity in identities:
 profile=identity['profile'];need(identity['source-sha256']==source,'source identity')
 need(identity['machine']=={'arm64':0xaa64,'x64':0x8664,'x86':0x14c,'x86-laa':0x14c}[profile],'machine identity')
 need(identity['large-address-aware']==(profile!='x86'),'LAA identity')
 raw=(N/f'native-system-time-v2-probe-{profile}.log').read_bytes();need(hashlib.sha256(raw).hexdigest()==identity['stdout-sha256'],'stdout identity')
 lines=raw.decode().splitlines();header=fields(lines[0]);footer=fields(lines[-1])
 need(header['profile']==profile and header['build']=='10.0.29683','native build')
 need(footer=={'cases':'205','successful':'21','failed':'0'},'native complete counts')
 rows=[fields(l)for l in lines if l.startswith('case ')];invalid=[fields(l)for l in lines if l.startswith('invalid ')]
 offsets=list(range(16))+list(range(4092,4096))
 need([(int(r['mode']),int(r['offset']))for r in rows]==[(m,o)for m in range(10)for o in offsets],'complete native axes')
 need([int(r['ordinal'])for r in rows+invalid]==list(range(205)),'native sequence')
 need([int(r['mode'])for r in invalid]==list(range(5)),'invalid pointer axes')
 for r in rows+invalid:
  need(r['exception']=='00000000' and r['last-error']=='4A2E19F7','native exception/last error')
 for r in rows:
  status=r['status'];need(r['outside']=='1','outside write')
  if status=='00000000':
   need(r['bounded']=='1' and int(r['before'],16)<=int(r['value'],16)<=int(r['after'],16),'clock bounds')
   need(r['bytes'][16:]=='A5'*8,'success output extent')
   need(bytes.fromhex(r['bytes'][:16])==int(r['value'],16).to_bytes(8,'little'),'little endian time')
  else:need(r['bytes']=='A5'*16,'failure mutated output')
  if int(r['offset'])%4:
   need(status=='80000002' and r['after0']==r['p0'] and r['after1']==r['p1'],'alignment precedence')
 need(collections.Counter(r['status']for r in rows)=={'00000000':21,'80000002':150,'C0000005':18,'80000001':11},'status partition')
 need([r['status']for r in invalid]==['C0000005','80000002','80000002','C0000005','C0000005'],'invalid status partition')
print('820 native system-time cases and four post-run producer identities verified')
