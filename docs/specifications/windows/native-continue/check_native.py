"""Independent captured-context continuation/APC oracle checker."""
from pathlib import Path
import hashlib,json
D=Path(__file__).resolve().parent;N=D/'native';PROFILES=['arm64','x64','x86','x86-laa'];SHA='da3664d9ac28c011cce31f04bba9b271bc2f53cbb28fe5310fee5294b89ddad0'
def load(path):return json.loads(path.read_text(encoding='utf-8-sig'))
def parse(text,name):return[dict(x.split('=',1)for x in line.split()[1:])for line in text.splitlines()if line.startswith(name+' ')]
def check():
 manifest=load(N/'manifest.json');names=set()
 for row in manifest['files']:
  assert row['path']not in names;names.add(row['path']);raw=(N/row['path']).read_bytes();assert len(raw)==row['bytes']and hashlib.sha256(raw).hexdigest()==row['sha256']
 assert{x.name for x in N.iterdir()if x.is_file()}==names|{'manifest.json'}
 assert hashlib.sha256((N/'native-continue-v2.cpp').read_bytes()).hexdigest()==SHA
 primary=load(D/'primary/sources.json');raw=(D/'primary'/primary['file']).read_bytes()
 assert len(raw)==primary['bytes']and hashlib.sha256(raw).hexdigest()==primary['sha256']
 assert primary['sha256']=='515ea365ad333fd3739f11228133dd4c6f26a958caffc36dcaacae512df19bd3'
 launch=load(N/'native-continue-v2-launch.json');assert launch['state']=='finished'and launch['exit-code']==0 and launch['source-before']==launch['source-after']and launch['source-before']['sha256']==SHA
 assert launch['runtime-modules-before']==launch['runtime-modules-after']and len(launch['runtime-modules-before'])==6
 assert [x['sha256']for x in launch['runtime-modules-before']][:2]==['502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f','ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e']
 assert len(launch['steps'])==16 and all(x['exit-code']==0 for x in launch['steps'])
 for row in launch['steps']:
  raw=(N/row['log']).read_bytes();assert len(raw)==row['bytes']and hashlib.sha256(raw).hexdigest()==row['sha256']
 assert[x['profile']for x in launch['outcomes']]==PROFILES
 groups=[]
 for profile,outcome in zip(PROFILES,launch['outcomes']):
  assert outcome['exit-code']==0 and outcome['executable-before']==outcome['executable-after']
  before=load(N/('native-continue-v2-'+profile+'-before.log'));after=load(N/('native-continue-v2-'+profile+'-after.log'));assert before==after and before['sha256']==outcome['executable-before']['sha256']
  assert before['machine']=={'arm64':0xaa64,'x64':0x8664,'x86':0x14c,'x86-laa':0x14c}[profile]and before['large-address-aware']==(profile!='x86')
  text=(N/outcome['log']).read_text();assert text.count('complete cases=13')==1 and 'unexpected-return'not in text
  header=dict(x.split('=',1)for x in text.splitlines()[0].split());assert header=={'profile':profile,'pointer':str(4 if profile.startswith('x86')else 8),'context-bytes':str({'arm64':912,'x64':1232,'x86':716,'x86-laa':716}[profile])}
  resumes=parse(text,'resume');checks=parse(text,'check');events=parse(text,'event');workers=parse(text,'worker');alerts=parse(text,'alert');captures=parse(text,'capture')
  assert len(resumes)==13 and len(checks)==26 and len(events)==12 and len(workers)==13 and len(alerts)==4
  assert all(x['wait']=='00000000'and x['exit']=='0'for x in workers)
  assert{int(x['case'])for x in alerts}=={6,7,8,12}and all(x['status']=='00000000'and x['seen']=='0'for x in alerts)
  assert len(captures)==(13 if profile.startswith('x86')else 0)
  for x in captures:assert x['pc']==x['expected-pc']and x['sp']==x['expected-sp']and x['flags']=='00010007'
  for number in range(13):
   resume=next(x for x in resumes if int(x['case'])==number);test={0:0,2:0,8:0,12:0,10:2,11:255}.get(number,1);assert int(resume['test'])==test
   wanted={0:[],1:[],2:[(0x1111,3)],3:[(0x1111,1)],4:[(0x1111,1),(0x2222,1)],5:[(0x1111,1),(0x2222,1),(0x3333,1)],6:[(0x1111,3)],7:[],8:[(0x1111,4)],9:[(0x4444,1)],10:[(0x1111,1)],11:[(0x1111,1)],12:[]}[number]
   got=[x for x in events if int(x['case'])==number];assert[(int(x['value'],16),int(x['phase']))for x in got]==wanted
   seen=sum(phase==1 for _,phase in wanted);assert int(resume['seen'])==seen and resume['last']==('0000ABCD'if number==9 else'0000C0DE')
   for index,x in enumerate(got):assert int(x['index'])==index and x['last']=='0000C0DE'and x['tid']==resume['tid']
   selected=[x for x in checks if int(x['case'])==number];assert[int(x['index'])for x in selected]==[0,1]
   for index,x in enumerate(selected):
    alerted=number in [8,12]and index==0;assert int(x['status'],16)==(0x101 if alerted else 0)
    assert int(x['before'])==(seen if index==0 else 0 if number==8 else len(wanted))
    assert int(x['after'])==(0 if alerted else len(wanted))and x['last']=='0000C0DE'
  groups.append({'profile':profile,'resumes':resumes,'checks':checks,'events':events})
 value={'native-resumes':52,'native-post-checks':104,'native-callbacks':48,'scope':'One Windows29683 ARM64 kernel/four ABI views; own captured contexts, private worker threads.','groups':groups}
 assert load(D/'cases.json')==value,'replay dataset differs from independent native records'
 return value
if __name__=='__main__':
 value=check();print(json.dumps({k:v for k,v in value.items()if k!='groups'},indent=2))
