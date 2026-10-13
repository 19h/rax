"""Independent provenance, input partition, ABI copy and native controls."""
from pathlib import Path
import collections,hashlib,itertools,json,re,sys
BASE=Path(sys.argv[1])if len(sys.argv)>1 else Path(__file__).parent
def require(ok,why):
 if not ok:raise AssertionError(why)
def j(p):
 b=p.read_bytes();return json.loads(b.decode('utf-16'if b.startswith(b'\xff\xfe')else'utf-8-sig'))
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def fields(s):return dict(re.findall(r'([\w-]+)=([^ ]*)',s))
def num(r,k,base=16):return int(r[k],base)
def lines(name):return (BASE/name).read_text().splitlines()
manifest=j(BASE/'evidence-manifest.json')['files']
require(set(manifest)=={str(p.relative_to(BASE))for p in BASE.rglob('*')if p.is_file()and p.name!='evidence-manifest.json'},'manifest-completeness')
for name,x in manifest.items():
 p=BASE/name;require(p.stat().st_size==x['bytes']and sha(p)==x['sha256'],'file-identity:'+name)
meta=j(BASE/'native-thread-token-producer-identities.json')
require(meta['kernel'][:3]==[10,0,29683],'native-kernel-build')
require('S-1-5-18'in meta['principal'],'native-principal')
require(len(meta['producers'])==13,'actual-producer-count')
require(meta['inspector-identity']['sha256']==sha(BASE/'inspect-thread-token-producers.py'),'native-inspector-source')
require([x['machine']for x in meta['native-libraries']]==[0xaa64,0x14c],'native-library-machine')
for p in meta['producers']:
 for name,key in ((p['source'],'source-identity'),(p['log'],'stdout-identity')):
  f=BASE/name;x=p[key];require(f.stat().st_size==x['bytes']and sha(f)==x['sha256'],'actual-native-'+key)
 e=p['executable-identity'];require(e['machine']=={'arm64':0xaa64,'x64':0x8664,'x86':0x14c,'x86-laa':0x14c}[p['profile']],'native-executable-machine')
 require(bool(e['characteristics']&0x20)==(p['profile']!='x86'),'native-executable-laa')
 stem={'probe-v5':'native-thread-token-open-probe-v5','pseudo':'native-thread-token-pseudo','boundary':'native-thread-token-boundary','fresh':'native-thread-token-open-fresh'}[p['family']]
 outcomes=j(BASE/(stem+('-arm64-outcome.json'if p['family']=='fresh'else'-outcomes.json')))
 row=outcomes if p['family']=='fresh'else next(x for x in outcomes if x['profile']==p['profile'])
 require(row['exit-code']==0,'producer-exit')
 require(row['source-hash'].lower()==p['source-identity']['sha256']and row['exe-sha256'].lower()==e['sha256']and row['exe-bytes']==e['bytes'],'producer-outcome-identity')
 if p['family']=='fresh':require(row['source-bytes']==p['source-identity']['bytes'],'fresh-source-size')
access=(0,1,8,0x20008,0x2001c,0xf01ff,0xffffffff,0x80000000,0x40000000,0x02000000,0x01000000,0x1000)
for profile in ('arm64','x64','x86','x86-laa'):
 width=8 if profile in ('arm64','x64')else 4;expected=[]
 for state in range(5):
  expected.extend((0,state,t,a,s,0)for t,a,s in itertools.product(range(24),access,(0,1,2,255)))
  expected.extend((1,state,0,a,s,m)for a,s,m in itertools.product((0,0x2001c,0xffffffff),(0,1),range(18)))
  if state==0:expected.extend((2,0,t,0x2001c,s,m)for t,s,m in itertools.product((1,2,3,4,5,6,11,18),(0,1),range(18)))
 ls=lines(f'native-thread-token-open-probe-v5-{profile}.log');rows=[fields(x)for x in ls if x.startswith('case=')]
 require(len(rows)==6588 and ls[-1].startswith('complete cases=6588'),'replay-count')
 require('owned-input-handles-closed=14 final-impersonation=0'in ls[-1],'producer-cleanup')
 identities={};absent=0
 for i,(r,partition)in enumerate(zip(rows,expected)):
  require(num(r,'case',10)==i and tuple(num(r,k,16 if k=='access'else 10)for k in ('family','state','thread','access','as-self','buffer'))==partition,'replay-partition')
  require(num(r,'width',10)==width and num(r,'exception')==0,'replay-width-exception')
  require(r['exact-token-and-input-handles']=='1','token-and-input-restoration')
  _,state,target,desired,self,mode=partition
  value=num(r,'input-thread');require(identities.setdefault(target,value)==value,'input-thread-identity')
  if target>=19:require(value==((1<<32)-(target-16)),'zero-extended-selector')
  if state:continue
  absent+=1;require(r['births']=='0'and r['new-handles']=='','absent-no-birth')
  status=0xc0000008 if target in (1,3)or target>=19 else 0xc0000024 if target in (2,11,12)else 0xc0000022 if target in (4,9,10,17)else 0xc000007c
  offset={0:128,1:None,2:None,3:129,4:132,5:136,6:4096,7:4096,8:4096,9:4096,10:4092,11:4092,12:4092,13:4092,14:4096-width,15:None,16:4094,17:4095}[mode]
  protect={6:2,7:1,8:0x104,9:0x102,10:1,11:0x104,12:2,13:0x102}.get(mode,4);flags=[4,protect];image=bytearray([0xa5])*8192
  if width==4 and mode!=1:
   if offset is None:status=0xc0000005
   elif offset+4>4096 and protect in (1,2,0x102):status=0xc0000005
   elif offset+4>4096 and protect==0x104:status=0x80000001;flags[1]=4
   else:image[offset:offset+4]=b'\0'*4
  require(num(r,'status')==status,'absent-kernel-copy-status')
  require([int(x,16)for x in r['protection'].strip(',').split(',')]==flags,'absent-guard-effect')
  require(bytes.fromhex(r['bytes0'])==image[112:176]and bytes.fromhex(r['bytes1'])==image[4064:4128],'absent-caller-bytes')
 require(absent==1548,'absent-partition')
 # Present-token captures must actually contradict an unconditional NO_TOKEN.
 positive=[r for r in rows if r['state']in ('2','3','4')and r['thread']=='0'and r['status']=='00000000']
 require(len(positive)>0 and all(r['births']=='1'and ':Token:'in r['new-handles']for r in positive),'present-token-native-birth')
 anonymous=[r for r in rows if r['state']=='1'and r['thread']=='0'and r['family']=='0'];require(all(r['status']=='C00000A6'for r in anonymous),'anonymous-native-control')

for profile in ('arm64','x64','x86','x86-laa'):
 width=8 if profile in ('arm64','x64')else 4
 ls=lines(f'native-thread-token-pseudo-{profile}.log')
 require(fields(ls[0])=={'width':str(width),'rtl-status-no-token-error':'1008'},'rtl-no-token-error')
 oracle=[fields(x)for x in ls if x.startswith('state=')]
 require(len(oracle)==2 and [x['oracle-status']for x in oracle]==['C000007C','00000000'],'pseudo-native-oracles')
 rows=[fields(x)for x in ls if x.startswith('case=')]
 require(len(rows)==40 and ls[-1]=='complete cases=40 final-impersonation=0','pseudo-count-cleanup')
 for i,(r,partition)in enumerate(zip(rows,itertools.product(range(2),range(2),range(3,8),range(2)))):
  state,mode,negative,self=partition
  require(int(r['case'])==i and tuple(int(r[k])for k in ('state','mode','negative','as-self'))==partition,'pseudo-partition')
  require(int(r['input-thread'],16)==((1<<(width*8 if mode==0 else 32))-negative),'pseudo-full-width-input')
  require(r['status']=='C0000008','pseudo-invalid-handle')
  require(num(r,'output-before')==int('A5'*width,16)and num(r,'output-after')==(int('A5'*width,16)if width==8 else 0),'pseudo-output-copy')
 rows=[fields(x)for x in lines(f'native-thread-token-boundary-{profile}.log')if x.startswith('case=')]
 require(len(rows)==24 and lines(f'native-thread-token-boundary-{profile}.log')[-1]=='complete cases=24','boundary-count')
 for i,(r,partition)in enumerate(zip(rows,itertools.product(range(2),(4093,4094,4095),range(4)))):
  target,offset,page=partition;protect=(2,1,0x104,0x102)[page]
  require(int(r['case'])==i and tuple(int(r[k])for k in ('thread','offset','page'))==partition,'boundary-partition')
  status=0xc000007c if target==0 else 0xc0000008
  if width==4:
   status=0x80000001 if page==2 else 0xc0000005
   if page==2:protect=4
  require(int(r['width'])==width and r['exception']=='00000000','boundary-width-exception')
  require(num(r,'status')==status,'boundary-status')
  require(num(r,'protection')==protect,'boundary-guard-priority')
  require(r['bytes']=='A5'*16,'boundary-no-partial-store')
ls=lines('native-thread-token-open-fresh-arm64.log')
require(ls[-1]=='complete cases=2 warmup=1','fresh-complete-count')
for case in range(3):
 group=[fields(x)for x in ls if f'case={case} 'in x]
 require(len(group)==6,'fresh-event-partition')
 image,entry,before,returned,after,summary=group
 require(num(image,'entry')==0x7ff8de2e1260 and num(image,'allocation')==0x7ff8de2e0000 and num(image,'type')==0x1000000,'fresh-installed-image')
 require(image['bytes'].startswith('810400D4C0035FD6'),'fresh-original-svc36-ret')
 require(entry['call']=='0'and entry['function']=='36'and num(entry,'pc')==num(image,'entry')+4,'fresh-entry-pc')
 args=[int(x,16)for x in entry['args'].strip(',').split(',')]
 require(len(args)==4 and args[:3]==[0xfffffffffffffffe,0x2001c,0]and args[3]==num(before,'address'),'fresh-original-arguments')
 require(returned['call']=='0'and returned['function']=='36'and returned['thread']==entry['thread']and num(returned,'pc')==num(entry,'lr')+4,'fresh-return-lr')
 require(returned['status']=='C000007C','fresh-return-no-token')
 require(before['phase']=='output-before'and after['phase']=='output-after'and before['address']==after['address']and before['size']==after['size']=='8'and before['data']==after['data'],'fresh-unchanged-output')
 require(summary['installed']==summary['done']==summary['calls']=='1'and summary['exit']=='77','fresh-debugger-stop')
 require(summary['warmup']==str(int(case==0)),'fresh-warmup-exclusion')
 if case:require(summary['exact-handle-set']=='1'and summary['handles-before']==summary['handles-after'],'fresh-parent-handles')
print(json.dumps({'native-producers':13,'mature-native-rows':26352,'absent-replay-rows':6192,'pseudo-native-rows':160,'absent-pseudo-replay-rows':80,'cross-page-native-replay-rows':96,'fresh-complete-first-call-cases':2,'warmup-excluded':1,'files':len(manifest)},sort_keys=True))
