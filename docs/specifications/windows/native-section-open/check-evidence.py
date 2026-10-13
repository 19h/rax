from pathlib import Path
import hashlib,json,re,struct,sys
ROOT=Path(__file__).parent
def require(ok,reason):
 if not ok:raise ValueError(reason)
def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def rows(p,prefix):
 return [dict(x.split('=',1)for x in l.split()if '='in x)for l in p.read_text().splitlines()if l.startswith(prefix)]
FROZEN={'fresh':('native-section-open-fresh-v3',2,'3f8335b3dff66629ba7a7c88c0d1e32cb01ff36bfa00106933e5f9fe5988faeb'),'probe':('native-section-open-probe-v3',5646,'30cec3641130887db7e46f01ae31c592d838f48c8c3baaef8faaf08a4d364a48'),'duplicate':('native-section-open-duplicate-v2',17760,'d6c34ea33a0d392b383631b0d8f83530d25b6452d4d3bfa4657119a28f05662d')}
MASKS=[0,1,2,4,8,0x10,0x20,0x40,0x80,0xD,0x1F,0xFFFF,0x10000,0x20000,0x40000,0x80000,0xF0000,0xF001F,0x100000,0x1F001F,0x01000000,0x02000000,0x03000000,0x04000000,0x08000000,0x80000000,0x40000000,0x20000000,0x10000000,0xC0000000,0xA0000000,0xE0000000,0xFFFFFFFF,0x0200000D,0x0100000D,0x0010000D,0x0002000D]
NAMES=['\\KnownDlls\\kernel32.dll','\\KNOWNDLLS\\KERNEL32.DLL','\\KnownDlls32\\kernel32.dll','\\KnownDlls\\ntdll.dll','\\KnownDlls32\\ntdll.dll','\\KnownDlls\\rax-absent-20261013','\\KnownDlls\\KnownDllPath','\\KnownDlls','kernel32.dll','KERNEL32.DLL','','\\','\\rax-missing\\kernel32.dll','\\KnownDlls\\kernel32.dll\\x','\\KnownDlls\\\\kernel32.dll','kernel32.dll\\x','\\KnownDlls\\kernel32.dll\\','\\KnownDllsChpe32\\kernel32.dll','\\KnownDllsChpe32\\ntdll.dll','KnownDlls32\\kernel32.dll','KnownDlls32\\ntdll.dll','KnownDllsChpe32\\kernel32.dll','KnownDlls\\kernel32.dll','Kernel32.Dll','rax-absent-20261013','.\\kernel32.dll','knowndlls32\\kernel32.dll']
def probe_inputs(width):
 seq=[]
 def add(f,n,r,a,g,ak=0,ok=0,nk=0,bk=0,al=None,length=None,maximum=None):
  le=len(NAMES[n].encode('utf-16le'))if length is None else length
  seq.append((f,n,r,a,g,ak,ok,nk,bk,(48 if width==8 else 24)if al is None else al,le,(le+2)&65535 if maximum is None else maximum))
 for a in MASKS:
  for n in [0,2,3,4,5,6,7,17,18]:
   for g in [0,0x40]:add(0,n,0,a,g)
 for r in range(15):
  for n in range(27):
   for g in [0,0x40,0x1000,0x1040]:add(1,n,r,0xD,g)
 for g in [0,0x42,0x60,0x140,0x240,0x440,0x840,0x1040,0x1FF2,0xFFFFFFFF]+[1<<b for b in range(32)]:
  for n in [0,5,6,7]:
   for a in [0,0xD,0x02000000,0xFFFFFFFF]:add(2,n,0,a,g)
 for ak in range(10):
  for ok in range(12):
   for nk in [0,1,2,5,6,8]:
    for al in [0,48 if width==8 else 24]:add(3,0,0,0xD,0x40,ak,ok,nk,0,al)
 for ok in range(12):
  for nk in [0,1,2,3,5,6,8,9]:
   for bk in [0,1,2,3,5,6,8,9]:add(4,0,0,0xD,0x40,0,ok,nk,bk)
 for n in [0,5,8,10]:
  for le in [0,1,2,46,48,50,65532,65534]:
   for maximum in [0,1,2,48,65535]:
    for bk in [0,1,3]:add(5,n,0,0xD,0x40,0,0,0,bk,length=le,maximum=maximum)
 return seq
def check_probe(p,width):
 rs=rows(p,'case=');inputs=probe_inputs(width);require(len(rs)==len(inputs)==5646,'probe row count')
 for ordinal,(r,expected)in enumerate(zip(rs,inputs)):
  keys=['family','name','root','access','flags','attributes','output','unicode','buffer','attributes-length','length','maximum']
  observed=tuple(int(r[k],16 if k in ('access','flags')else 10)for k in keys)
  require(int(r['case'])==ordinal and observed==expected,'probe input partition')
  require(int(r['width'])==width and r['exception']=='00000000','probe ABI/exception')
  require(r['exact-section-and-input-handles']=='1','probe cleanup')
  status=int(r['status'],16);born=int(r['birth-count']);require(born==(1 if status==0 else 0),'probe birth/status')
  require(len(r['protection'].rstrip(',').split(','))==8,'probe protection extent')
  if r['observed']=='1':require(len(bytes.fromhex(r['output-bytes']))==width+8,'probe output extent')
  else:require(r['output-bytes']=='','probe unobserved output')
  if born:
   b=r['births'].rstrip(',').split(':');require(len(b)==10 and b[1]=='Section'and int(b[0],16)==int(r['result'],16),'probe typed publication')
   require(int(b[3])==bool(int(r['flags'],16)&2),'probe inherit flag')
   name=bytes.fromhex(b[9]).decode('utf-16le');require(name.startswith(('\\KnownDlls\\','\\KnownDlls32\\','\\KnownDllsChpe32\\')),'probe selected canonical name')
   if b[4]=='00000000':require(int(b[5])==(24 if width==8 else 16)and int(b[6],16)==0 and int(b[7],16)==0x01800000 and int(b[8],16)>0,'probe image basic metadata')
   else:require(b[4]=='C0000022'and int(b[2],16)&1==0 and int(b[5])==(0xFFFFFFF8 if width==4 else 0) and all(int(b[k],16)==0 for k in [6,7,8]),'probe query-right denial')
  else:require(r['births']=='','probe unexpected birth detail')
 # Independent API-type and direct-root CHPE controls.
 def case(f,n,r,a,g):return next(x for x in rs if tuple(int(x[k],16 if k in ('access','flags')else 10)for k in ['family','name','root','access','flags'])==(f,n,r,a,g))
 require(case(0,0,0,0x02000000,0x40)['births'].split(':')[2]=='0002000F','probe captured subject ceiling')
 require(case(2,0,0,0xD,0x10000)['status']=='00000000','probe kernel-handle flag')
 require(case(1,23,2,0xD,0)['status']=='C0000034','probe section case sensitivity')
 direct=case(1,8,3,0xD,0x40);expanded=case(1,19,1,0xD,0x40)
 target=lambda r:bytes.fromhex(r['births'].rstrip(',').split(':')[9]).decode('utf-16le')
 require(target(direct)==('\\KnownDllsChpe32\\kernel32.dll'if width==4 else'\\KnownDlls32\\kernel32.dll'),'probe direct-root redirect')
 require(target(expanded)=='\\KnownDlls32\\kernel32.dll','probe root-expansion boundary')
 special=next(r for r in rs if r['family']=='3'and r['attributes']=='0'and r['output']=='9'and r['unicode']=='0'and r['attributes-length']==str(48 if width==8 else 24))
 require(special['status']==('C0000005'if width==4 else'80000001')and special['protection'].split(',')[2]==('102'if width==4 else'2'),'probe read-only guard policy')
def check_duplicate(p,width):
 rs=rows(p,'case=');restored=rows(p,'restored ');require(len(rs)==len(restored)==17760,'duplicate row count')
 expected=[(parent,source,desired,options,flags)for parent in range(3)for source in [1,0xD,0x20000,0x02000000]for desired in MASKS for options in [0,1,2,3,4,5,6,7,8,0xFFFFFFFF]for flags in [0,1,2,3]]
 for i,(r,e,restore)in enumerate(zip(rs,expected,restored)):
  observed=tuple(int(r[k],10 if k=='parent'else 16)for k in ['parent','source-access','desired','options','source-flags'])
  require(int(r['case'])==int(restore['case'])==i and observed==e,'duplicate input partition')
  require(int(r['width'])==width and r['exception']=='00000000','duplicate ABI/exception')
  require(restore['exact-section-handles']=='1','duplicate cleanup')
  require(r['ok']in ('0','1')and r['source-live']in ('0','1'),'duplicate result booleans')
  if r['ok']=='1':require(int(r['error'])==0xBADC0DE and int(r['output'],16)>0,'duplicate successful publication')
  else:require(int(r['grant'],16)==0 and int(r['error'])in (5,87,1314),'duplicate failure detail')
def check():
 require(sys.flags.optimize==0,'optimized verification forbidden')
 identity=json.loads((ROOT/'native-section-producer-identities.json').read_text());require(identity['native-machine']==0xAA64 and 'S-1-5-18'in identity['principal'],'native host/subject identity')
 require(identity['kernel'][:3]==[10,0,29683],'captured kernel identity')
 inspector=ROOT/'inspect-section-producers.py';require(inspector.stat().st_size==identity['inspector-identity']['bytes']and digest(inspector)==identity['inspector-identity']['sha256'],'actual inspector source identity')
 expected_libraries=[(4609312,'502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f',0xAA64),(1849976,'ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e',0x14C)]
 require([(x['bytes'],x['sha256'],x['machine'])for x in identity['native-libraries']]==expected_libraries,'captured NTDLL identities')
 ps=identity['producers'];require(len(ps)==9,'producer count');seen=set()
 require({(x['family'],x['profile'])for x in ps}=={('fresh','arm64')}|{(f,p)for f in ('probe','duplicate')for p in ('arm64','x64','x86','x86-laa')},'complete producer profile partition')
 for item in ps:
  family=item['family'];profile=item['profile'];stem,count,source_hash=FROZEN[family];key=(family,profile);require(key not in seen,'duplicate producer');seen.add(key)
  require(item['source']==stem+'.cpp'and item['log']==stem+'-'+profile+'.log','producer naming')
  source=ROOT/item['source'];p=ROOT/item['log'];require(digest(source)==source_hash==item['source-identity']['sha256'],'frozen source identity')
  require(source.stat().st_size==item['source-identity']['bytes'],'source byte count')
  require(digest(p)==item['stdout-identity']['sha256']and p.stat().st_size==item['stdout-identity']['bytes'],'stdout identity')
  e=item['executable-identity'];require(e['machine']=={'arm64':0xAA64,'x64':0x8664,'x86':0x14C,'x86-laa':0x14C}[profile]and bool(e['characteristics']&0x20)==(profile!='x86'),'actual executable ABI/LAA')
  require('failure='not in p.read_text()and f'complete cases={count}'in p.read_text(),'producer completion')
  width=8 if profile in ('arm64','x64')else 4
  if family=='probe':check_probe(p,width)
  elif family=='duplicate':check_duplicate(p,width)
  else:
   cases=rows(p,'case=');require(len(cases)==3 and cases[0]['warmup']=='1'and all(x['done']=='1'and x['exit']=='77'and x['calls']=='1'for x in cases),'fresh debugger completion')
   require(all(x['exact-handle-set']=='1'for x in cases[1:]),'fresh parent restoration')
   entry=rows(p,'entry ');returned=rows(p,'return ');require(len(entry)==len(returned)==3,'fresh entry/return count')
   require(all(x['function']=='55'for x in entry),'fresh original service entry');require(all(x['function']=='55'and x['status']=='00000000'for x in returned),'fresh original service return')
   require(all(int(r['pc'],16)==int(e['lr'],16)+4 for e,r in zip(entry,returned)),'fresh true LR')
   images=rows(p,'image ');require(len(images)==3,'fresh image count')
   require(all(int(e['pc'],16)==int(i['entry'],16)+4 and (int.from_bytes(bytes.fromhex(i['bytes'])[:4],'little')>>5)&65535==55 for e,i in zip(entry,images)),'fresh original SVC')
   require(all(len(e['args'].rstrip(',').split(','))==3 and e['args'].split(',')[1]=='D'for e in entry),'fresh original arguments')
   attrs=[r for r in rows(p,'bytes ')if r['phase']=='object-attributes'];require(len(attrs)==3 and all(len(bytes.fromhex(r['data']))==48 and bytes.fromhex(r['data'])[:4]==(48).to_bytes(4,'little')and bytes.fromhex(r['data'])[24:28]==(0x40).to_bytes(4,'little')and bytes.fromhex(r['data'])[32:]==bytes(16)for r in attrs),'fresh original attributes')
   roots=[r for r in rows(p,'object ')if r['phase']=='root-before'and r['class']=='0'];require(len(roots)==3 and all(int.from_bytes(bytes.fromhex(r['data'])[4:8],'little')==3 for r in roots),'fresh original root grant')
   opened=[r for r in rows(p,'object ')if r['phase']=='opened-section'and r['class']=='0'];require(len(opened)==3 and all(int.from_bytes(bytes.fromhex(r['data'])[4:8],'little')==0xD for r in opened),'fresh exact section grant')
   info=rows(p,'section ');require(len(info)==3 and all(r['status']=='00000000'and int(r['need'])==24 and int.from_bytes(bytes.fromhex(r['data'])[8:12],'little')==0x01800000 and int.from_bytes(bytes.fromhex(r['data'])[16:24],'little')==0x171000 for r in info),'fresh section basic metadata')
  outcome_name=stem+'-'+profile+'-outcome.json'if family=='fresh'else stem+'-outcomes.json'
  outcomes=json.loads((ROOT/outcome_name).read_text());outcomes=[outcomes]if isinstance(outcomes,dict)else outcomes
  outcome=outcomes[0]if family=='fresh'else next(x for x in outcomes if x['profile']==profile)
  require(outcome['exit-code']==0 and outcome['source-hash'].lower()==source_hash and outcome['exe-sha256'].lower()==e['sha256']and outcome['exe-bytes']==e['bytes'],'actual build/run outcome')
  build=(ROOT/(stem+'-'+profile+'-build.log')).read_text();require(stem+'.cpp'in build and 'warning'not in build.lower()and 'error'not in build.lower(),'clean native compiler output')
 for ref in json.loads((ROOT/'references/sources.json').read_text()):
  p=ROOT/'references'/ref['file'];require(p.stat().st_size==ref['bytes']and digest(p)==ref['sha256'],'primary reference identity')
 print('verified native section producers=9 open-rows=22584 duplicate-rows=71040 fresh-first-calls=2')
if __name__=='__main__':check()
