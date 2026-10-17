from pathlib import Path
import argparse,gzip,hashlib,json,struct
D=Path(__file__).resolve().parent
N=D/'native'
parser=argparse.ArgumentParser();parser.add_argument('--image-cache',type=Path,required=True);args=parser.parse_args()
IMAGE_CACHE=args.image_cache
def require(v,m):
 if not v:raise ValueError(m)
require(__debug__,'optimized checker rejected')
def load(name):return json.loads((N/name).read_text())
def handles(a):require(a['handles-before']==a['handles-after'],'handle count changed')
def normalize(raw,destination,machine=0xaa64):
 b=bytearray(raw);pe=struct.unpack_from('<I',b,60)[0]
 require(b[:2]==b'MZ' and b[pe:pe+4]==b'PE\0\0','PE signature')
 require(struct.unpack_from('<H',b,pe+4)[0]==machine,'image machine identity')
 require(struct.unpack_from('<I',b,pe+80)[0]==len(b),'PE size')
 source=struct.unpack_from('<Q',b,pe+48)[0];delta=(destination-source)&((1<<64)-1)
 at,size=struct.unpack_from('<II',b,pe+24+112+40);end=at+size;count=0
 require(end<=len(b) and size<=1<<20,'relocation extent')
 while at<end:
  page,n=struct.unpack_from('<II',b,at);require(n>=8 and n%2==0 and at+n<=end,'relocation block')
  for p in range(at+8,at+n,2):
   entry=struct.unpack_from('<H',b,p)[0];kind=entry>>12
   require(kind in [0,10],'relocation type')
   if kind:
    pos=page+(entry&4095);require(pos+8<=len(b),'relocation target')
    value=struct.unpack_from('<Q',b,pos)[0];struct.pack_into('<Q',b,pos,(value+delta)&((1<<64)-1));count+=1
  at+=n
 struct.pack_into('<Q',b,pe+48,destination)
 return bytes(b),count
manifest=json.loads((D/'native-inputs.json').read_text());seen=set()
for row in manifest['files']:
 require(row['path'] not in seen,'duplicate native path');seen.add(row['path']);raw=(D/row['path']).read_bytes()
 require(len(raw)==row['bytes'] and hashlib.sha256(raw).hexdigest()==row['sha256'],'native input digest '+row['path'])

for row in json.loads((D/'external-images.json').read_text())['files']:
 raw=(IMAGE_CACHE/Path(row['path']).name).read_bytes()
 require(len(raw)==row['bytes'] and hashlib.sha256(raw).hexdigest()==row['sha256'],'external mapped image digest')
sizes={'ntdll.dll':0x473000,'kernelbase.dll':0x6e6000,'kernel32.dll':0x171000}
a=load('native-image-extension-partials.json');handles(a)
expected={(name,flag,n)for name,size in sizes.items()for flag in [0,0x800000]for n in [0,4096,65536,size-4096,size,size+4096]}
require({(r['name'],r['flags'],r['requested'])for r in a['rows']}==expected and len(a['rows'])==36,'partial matrix')
for r in a['rows']:
 size=sizes[r['name']];require(r['image-size']==size,'partial image identity')
 if r['requested']==size+4096:require(r['status']==0xc000001f,'oversize status');continue
 require(r['status']==(0x40000036 if r['flags'] else 0x40000003) and r['unmap-status']==0,'partial map status')
 present=r['requested'] in [0,size];q=r['extension'];require((q['rva'],q['size'])==((size,4096)if present else(0,0)),'partial extension')
 require(struct.unpack_from('<I',bytes.fromhex(r['image-information']['bytes']),16)[0]&(1<<6)==(64 if present else 0),'partial image flag')
a=load('native-image-extension-large.json');handles(a)
require(len(a['rows'])==18,'large count')
require({(r['name'],r['flags'],r['requested'])for r in a['rows']}=={(n,f,z)for n,s in sizes.items()for f in [0x20000000,0x20100000]for z in [0,4096,s]},'large axes')
for r in a['rows']:require(r['unmap-status']==0 and r['extension']['rva']==0 and r['extension']['size']==0,'large absent extension')
a=load('native-image-extension-lifetime.json');handles(a)
require(len(a['rows'])==90,'lifetime count')
expected={(name,flag,op,v)for name in sizes for flag in [0,0x800000]for op,values in [('protect',[1,2,4,8,16,32,64,128,0x120]),('unmap',[0,-1,1,4095]),('free',[0x4000,0x8000])]for v in values}
require({(r['name'],r['flags'],r['op'],r['value'])for r in a['rows']}==expected,'lifetime axes')
for r in a['rows']:
 expected_status=0 if r['op']=='unmap' else 0xc000002d if r['op']=='protect' else 0xc000001b if r['value']==0x4000 else 0xc000009f
 require(r['operation-status']==expected_status,'lifetime status')
 require(r['released']['state']==65536,'released tail')
 if r['op']=='protect':require(r['old']==1 and r['before']==r['after'],'protection refusal outputs')
a=load('native-image-extension-lifetime-v2.json');handles(a)
require(len(a['rows'])==252,'lifetime v2 count')
for r in a['rows']:
 if r['op']=='protect':
  require(r['operation-status']==0xc000002d and r['old']==1,'unaligned protection refusal outputs')
  require(r['before']==r['after'],'unaligned protection changes state')
  require(r['returned-base']==r['base']+r['size']+r['value'][1] and r['returned-size']==r['value'][2],'failure changed caller span')
 require(r['released']['state']==65536,'v2 release tail')
for family,machine in [('native-image-extension-normalization.json',0xaa64),('native-image-extension-projected-normalization.json',0x8664)]:
 a=load(family);handles(a);require(len(a['rows'])==9,'normalization count')
 for name,size in sizes.items():
  rows=[r for r in a['rows']if r['name']==name];require([r['index']for r in rows]==[0,1,2],'normalization axes')
  require(len({r['base']for r in rows})==3,'independent live views')
  raws=[]
  for r in rows:
   raw=gzip.decompress((IMAGE_CACHE/r['archive']).read_bytes());require(len(raw)==size and hashlib.sha256(raw).hexdigest()==r['sha256'],'full image digest');raws.append(raw)
  preferred=rows[0]['preferred'];one,count=normalize(raws[1],preferred,machine);two,count2=normalize(raws[2],preferred,machine)
  require(one==two and count==count2,'independent native rebase closure')
  q=rows[2]['normalization'];require(count==q['fixups'] and hashlib.sha256(one).hexdigest()==q['normalized-sha256'],'normalized identity')
  diffs=[i for i,(x,y)in enumerate(zip(raws[0],one))if x!=y]
  require(len(diffs)=={'ntdll.dll':32,'kernelbase.dll':12,'kernel32.dll':8}[name],'guard adjustment count')
  require(sum(y-x for x,y in q['difference-ranges'])==len(diffs) and set(diffs)=={i for x,y in q['difference-ranges']for i in range(x,y)},'guard adjustment ranges')
for name in ['native-image-extension-views.json','native-image-extension-machine-views.json']:
 a=load(name);handles(a)
 for r in a['images']:require(r['unmap-status']==0 and r['close-status']==0,'owned view cleanup')
def record_text(path):
 raw=path.read_bytes()
 return raw.decode('utf-16') if raw[:2] in [b'\xff\xfe',b'\xfe\xff'] else raw.decode('utf-8-sig')
def fields(line):return dict(part.split('=',1)for part in line.split()if '='in part)
identities=load('image-extension-owner-v3-final-producer-identities.json')
require({r['profile']for r in identities}=={'arm64','x64','x86','x86-laa'},'native producer profiles')
for identity in identities:
 profile=identity['profile'];stem='native-image-extension-owner-probe-v3-'+profile
 require(identity['source-sha256']==hashlib.sha256((N/'native-image-extension-owner-probe-v3.cpp').read_bytes()).hexdigest(),'actual source identity')
 require(identity['machine']=={'arm64':0xaa64,'x64':0x8664,'x86':0x14c,'x86-laa':0x14c}[profile],'actual PE machine')
 require(identity['large-address-aware']==(profile!='x86'),'actual PE LAA')
 raw=(N/(stem+'.log')).read_bytes();require(hashlib.sha256(raw).hexdigest()==identity['stdout-sha256'],'actual stdout identity')
 text=raw.decode();lines=text.splitlines();header=fields(lines[0]);footer=fields(lines[-1])
 require(header['profile']==profile and header['build']=='10.0.29683','native build/profile')
 cases=[fields(line)for line in lines if line.startswith('case ')]
 require([int(row['ordinal'])for row in cases]==list(range(48)),'ordered native case partition')
 require({int(row['flags'],16)for row in cases}=={0,0x800000,0x20000000,0x20800000},'native flag axis')
 require(footer['failed']=='0' and footer['cases']=='48' and footer['closed']=='3' and footer['mapped']==footer['unmapped'] and footer['handles-before']==footer['handles-after'],'native cleanup footer')
 protects=[fields(line)for line in lines if line.startswith('protect ')]
 require(len(protects)=={'arm64':108,'x64':324,'x86':0,'x86-laa':0}[profile],'native protection partition')
 for row in protects:require(row['status']=='C000002D' and row['old']=='00000001' and row['size']=='1','native protection outputs')
 pre=json.loads(record_text(N/(stem+'-identity.json')))
 require(pre['sha256']==identity['sha256'] and pre['bytes']==identity['bytes'],'post-run executable identity')
print('native image-extension inputs, matrices, lifetimes and independent rebase closure verified')
