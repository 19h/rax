"""Independent native producer, partition, byte and lifecycle checks."""
from pathlib import Path
import collections,hashlib,itertools,json,re,sys
BASE=Path(sys.argv[1])if len(sys.argv)>1 else Path(__file__).parent

def require(ok,reason):
 if not ok:raise AssertionError(reason)
def read_json(p):
 b=p.read_bytes();return json.loads(b.decode('utf-16'if b.startswith(b'\xff\xfe')else'utf-8-sig'))
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def fields(line):return dict(re.findall(r'([\w-]+)=([^ ]+)',line))
def n(row,key,radix=16):return int(row[key],radix)
def lines(p):return p.read_text().splitlines()
manifest=read_json(BASE/'evidence-manifest.json')
for name,item in manifest['files'].items():
 p=BASE/name;require(p.is_file(),'missing-evidence-file:'+name);require(p.stat().st_size==item['bytes'] and sha(p)==item['sha256'],'file-identity:'+name)
meta=read_json(BASE/'native-scheduler-slot-producer-identities.json')
require(meta['kernel'][:3]==[10,0,29683],'native-kernel-build')
require('S-1-5-18'in meta['principal'],'native-principal')
require(len(meta['producers'])==26,'actual-producer-count')
require([p['machine']for p in meta['native-libraries']]==[0xaa64,0x14c],'native-library-machines')
require(meta['inspector-identity']['sha256']==sha(BASE/'inspect-scheduler-slot-producers.py'),'native-inspector-source')
for p in meta['producers']:
 family,profile=p['family'],p['profile'];source=BASE/p['source'];log=BASE/p['log']
 for path,key in ((source,'source-identity'),(log,'stdout-identity')):
  identity=p[key];require(path.stat().st_size==identity['bytes'] and sha(path)==identity['sha256'],'actual-native-'+key)
 e=p['executable-identity'];require(e['machine']=={'arm64':0xaa64,'x64':0x8664,'x86':0x14c,'x86-laa':0x14c}[profile],'native-executable-machine')
 require(bool(e['characteristics']&0x20)==(profile!='x86'),'native-executable-laa')
 require(e['bytes']>100000 and len(e['sha256'])==64,'native-executable-identity')
 if family not in ('fresh-v2','sequence'):
  outcomes=read_json(BASE/f'native-scheduler-thread-slot-{family}-outcomes.json')
  row=next(x for x in outcomes if x['profile']==profile)
  require(row['exit-code']==0,'producer-exit')
  require(row['source-hash'].lower()==p['source-identity']['sha256'],'producer-source-hash')
  require(row['exe-sha256'].lower()==e['sha256'] and row['exe-bytes']==e['bytes'],'producer-executable-outcome')
lengths=(0,1,4,7,8,11,12,16,23,24,25,32,4097,0xffffffff)
for profile in ('arm64','x64','x86','x86-laa'):
 width=8 if profile in ('arm64','x64')else 4;size=24 if width==8 else 12
 expected=[]
 for b,t,a,o,s in itertools.product(range(2),range(14),(0,1,2,3,0xffffffff),range(6),range(4)):expected.append((0,b,t,size,0,a,o,s,0))
 for b,l,m,a,o in itertools.product(range(2),lengths,range(14),(0,1,2,0xffffffff),range(3)):expected.append((1,b,0,l,m,a,o,0,0))
 for b,t,l,m in itertools.product(range(2),(1,3,4,10),lengths,range(14)):expected.append((2,b,t,l,m,0,0,0,0))
 for b,a,o,s,pad in itertools.product(range(2),(0,1,2,3,0xffffffff),range(3),range(4),range(2)):expected.append((3,b,0,size,0,a,o,s,pad))
 rows=[fields(x)for x in lines(BASE/f'native-scheduler-thread-slot-probe-v2-{profile}.log')if x.startswith('case=')]
 require(len(rows)==9872,'replay-count')
 owner_handles={};thread_handles={};slot_values=set()
 for ordinal,(r,partition)in enumerate(zip(rows,expected)):
  keys=('family','bound','thread','length','buffer','action','owner','slot-mode','padding-mode')
  require(tuple(n(r,k,16 if k in ('length','action')else 10)for k in keys)==partition and n(r,'case',10)==ordinal,'replay-partition')
  require(n(r,'width',10)==width and n(r,'exception')==0,'replay-width-exception')
  require(r['exact-handle-set']=='1' and r['final-slot']=='0' and r['handles-before']==r['handles-after'],'replay-handle-restoration')
  _,bound,thread,length,mode,action,owner,supplied,pad=partition
  oh=n(r,'input-owner');th=n(r,'input-thread');before=n(r,'before-slot');after=n(r,'after-slot')
  require(owner_handles.setdefault(owner,oh)==oh and thread_handles.setdefault(thread,th)==th,'replay-input-identities')
  require(bool(before)==bool(bound),'replay-before-binding')
  if before:slot_values.add(before)
  if after:slot_values.add(after)
  require(n(r,'input-slot')==(0 if supplied==0 else 1 if supplied==1 else(1<<(width*8))-1 if supplied==2 else before),'replay-input-slot')
  offset={0:128,1:None,2:None,3:129,4:132,5:4096,6:4096,7:4096,8:4096,9:4092,10:4092,11:4092,12:4096-size,13:None}[mode]
  protection={5:2,6:1,7:0x104,8:0x102,9:1,10:0x104,11:2}.get(mode,4)
  flags=[4,protection]
  def probe(bytes_count,write):
   if bytes_count==0:return 0
   if offset is None:
    if width==8 and mode==2:return 0x80000002
    return 0xc0000005
   if width==8 and offset%4:return 0x80000002
   if offset+bytes_count<=4096:return 0
   if protection&0x100:flags[1]=protection&~0x100;return 0x80000001
   if protection==1 or(write and protection==2):return 0xc0000005
   return 0xc0000005 if offset+bytes_count>8192 else 0
  status=0;kernel=0;binding=before
  if width==8:status=probe(length,True)
  if not status and length!=size:status=0xc0000004
  if not status and width==4:status=probe(size,False)
  if not status:
   kernel=0xc0000008 if thread in (1,3)else 0xc0000024 if thread in (2,10,11)else 0xc0000022 if thread in (4,6,7,8,12)else 0xc00000bb if action>1 else 0
   if not kernel and action==0:kernel=0xc0000008 if owner in (1,2)else 0xc0000024 if owner in (3,4,5)else 0xc00000bb if bound else 0
   if kernel==0:
    binding=0 if action==1 else after
    require(action!=0 or after!=0,'replay-published-binding')
   status=kernel
   if width==4:
    copy=probe(size,True)
    if copy:status=copy
  require(n(r,'status')==status,'replay-status-order')
  require(after==binding,'replay-binding-effect')
  require([int(x,16)for x in r['protection'].strip(',').split(',')]==flags,'replay-guard-effect')
  image=bytearray([0xa5])*8192
  if offset is not None:
   record=bytearray(size);record[:4]=action.to_bytes(4,'little');at=8 if width==8 else 4
   if width==8 and pad:record[4:8]=b'\xff'*4
   record[at:at+width]=oh.to_bytes(width,'little');record[at+width:at+2*width]=n(r,'input-slot').to_bytes(width,'little');image[offset:offset+size]=record
   if width==4 and pad:image[offset+12:offset+16]=b'\xff'*4
   if status==0 and action==0:image[offset+at+width:offset+at+2*width]=after.to_bytes(width,'little')
  require(bytes.fromhex(r['bytes0'])==image[112:208] and bytes.fromhex(r['bytes1'])==image[4064:4160],'replay-caller-bytes')
 require(len(slot_values)==1,'replay-single-owner-slot-identity')
 require(owner_handles[1]==0 and owner_handles[2]==0x1234 and thread_handles[1]==0 and thread_handles[3]==0x1234,'replay-invalid-selector-identities')
for family in ('resources-v2','pool','capacity','boundaries','boundaries-v2'):
 for profile in ('arm64','x64','x86','x86-laa'):
  ls=lines(BASE/f'native-scheduler-thread-slot-{family}-{profile}.log');require(ls[-1]=='complete rounds=3 warmup-excluded=1','lifecycle-complete')
  rounds=[fields(x)for x in ls if x.startswith('round-complete=')];require(len(rounds)==3 and all(r['exact-handle-set']=='1' and r['before']==r['after'] and r['final-slot']=='0'for r in rounds),'lifecycle-handle-restoration')
  calls=[fields(x)for x in ls if x.startswith('call ')];maps=[fields(x)for x in ls if x.startswith('map ')]
  require(all(n(r,'status')==0xc0000022 for r in calls if r['label']=='zero-grant'),'owner-grant-required')
  require(all(n(r,'status')==0 for r in calls if r['label']=='tagged-owner'),'owner-tag-bits')
  for label,state in (('free-before-close-active',0x1000),('free-before-close-released',0x1000),('free-before-close-final',0x10000),('active-close-retained',0x1000),('active-close-final',0x10000)):
   selected=[r for r in maps if r['label']==label];require(len(selected)==3 and all(n(r,'state')==state for r in selected),'mapping-reference-lifetime:'+label)
  count=130 if family=='capacity'else 66 if family=='pool'else 2
  workers=[fields(x)for x in ls if x.startswith('worker-ready')];require(len(workers)==3*count,'worker-partition')
  for start in range(0,len(workers),count):
   part=workers[start:start+count];base=n(part[0],'binding')
   require(base%65536==0,'mapping-allocation-alignment')
   require(all(n(r,'index',10)==i and n(r,'binding')==(base+i*64 if i<128 else 0)for i,r in enumerate(part)),'slot-pool-layout')
  if family=='capacity':
   assigned=[r for r in calls if r['label']=='worker-self-assign'];require(collections.Counter(r['status']for r in assigned)=={'00000000':384,'C000009A':6},'slot-capacity-status')
   require(all(n(r,'status')==0xc000009a and n(r,'after')==0 for r in calls if r['label']=='multi-main'),'capacity-main-binding')
  if family=='pool':
   require(all(r['bytes']=='5A'+'00'*63 for r in maps if r['label']=='multi-released-bytes'),'free-preserves-slot-bytes')
   require(all(r['bytes']=='00'*64 for r in maps if r['label']=='multi-reassigned-bytes'),'assign-clears-slot-bytes')
  if family in ('resources-v2','boundaries','boundaries-v2'):
   for label,state in (('multi-after-close',0x1000),('multi-one-binding',0x1000),('multi-no-binding',0x10000)):
    selected=[r for r in maps if r['label']==label];require(len(selected)==3 and all(n(r,'state')==state for r in selected),'multi-thread-exit-lifetime:'+label)
  if family=='boundaries-v2':
   remote=[r for r in calls if r['label']=='remote-matrix'];require(len(remote)==90,'remote-partition')
   expected_status=[0xc00000bb,0xc0000008,0xc0000008,0xc0000024,0xc0000024,0xc0000024 if profile in ('arm64','x64')else 0xc0000008]
   for at,row in enumerate(remote):
    require(n(row,'action',10)==(0,1,2,3,0xffffffff)[(at%30)//6],'remote-action-order')
    require(n(row,'status')==(expected_status[at%6]if at%30<6 else 0xc00000bb),'remote-status-order')
   require(all(n(r,'state')==0x10000 for r in maps if r['label']=='unmap-after-reassign'),'explicit-unmap-stale-address')
for family in ('fresh-v2','sequence'):
 ls=lines(BASE/f'native-scheduler-thread-slot-{family}-arm64.log');require(ls[-1]=='complete cases=2 warmup=1','fresh-complete')
 cases=[fields(x)for x in ls if x.startswith('case=')];require([n(r,'case',10)for r in cases]==[0,1,2],'fresh-partition')
 require(all(r['installed']=='1' and r['done']=='1' and r['exit']=='77' and r['exact-handle-set']=='1'for r in cases[1:]),'fresh-owned-child-restoration')
 entries=[fields(x)for x in ls if x.startswith('entry ')];returns=[fields(x)for x in ls if x.startswith('return ')];require(len(entries)==len(returns)==(3 if family=='fresh-v2'else 24),'fresh-call-count')
 for e,r in zip(entries,returns):
  require(e['function']==r['function']=='13' and e['case']==r['case'] and e['call']==r['call'] and n(r,'pc')==n(e,'lr')+4,'fresh-original-svc-true-lr')
  require(n(e,'pc')==0x7ff8de2e10d4,'fresh-original-entry')
 owners=[fields(x)for x in ls if x.startswith('owner ')];require(len(owners)==len(entries) and all(r['grant']=='00000001' and r['type-match']=='1' and r['duplicate-closed']=='1'for r in owners),'fresh-owned-resource-type')
 if family=='sequence':
  pages=[fields(x)for x in ls if 'phase=original-slot-page' in x];require(len(pages)==24 and all(r['size']=='4096' and r['data']=='00'*4096 for r in pages),'fresh-full-page-zero')
  require([r['status']for r in returns]==['00000000','C00000BB','C00000BB','C00000BB','00000000','C00000BB','00000000','00000000']*3,'cold-sequence-status')
 else:require(all(r['status']=='00000000'for r in returns),'fresh-first-assignment-success')
print(json.dumps({'checked':'native-scheduler-thread-slot','actual-producers':26,'replay-rows':39488,'fresh-first-calls':2,'complete-sequence-cases':2,'all-controlled-rounds':3,'ordinary-guest-exit-proof':False},indent=2))
