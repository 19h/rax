"""Independent native abandonment of an APC at its exact interrupted context."""
from pathlib import Path
import hashlib,json
D=Path(__file__).resolve().parent;N=D/'native-supplement';SHA='841df7641840887b27da6b10543da65922cce8997c97274a649b11f6da103e87'
def load(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def parse(text,name):return[dict(x.split('=',1)for x in line.split()[1:])for line in text.splitlines()if line.startswith(name+' ')]
def check():
 manifest=load(N/'manifest.json');names=set()
 for row in manifest['files']:
  assert row['path']not in names;names.add(row['path']);raw=(N/row['path']).read_bytes();assert(len(raw),hashlib.sha256(raw).hexdigest())==(row['bytes'],row['sha256'])
 assert{x.name for x in N.iterdir()if x.is_file()}==names|{'manifest.json'}
 assert hashlib.sha256((N/'native-continue-v3.cpp').read_bytes()).hexdigest()==SHA
 launch=load(N/'native-continue-v3-launch.json');assert launch['state']=='finished'and launch['exit-code']==0 and launch['source-before']==launch['source-after']and launch['source-before']['sha256']==SHA
 assert launch['runtime-modules-before']==launch['runtime-modules-after']==load(D/'native/native-continue-v2-launch.json')['runtime-modules-before']
 assert len(launch['steps'])==16 and all(x['exit-code']==0 for x in launch['steps'])
 for row in launch['steps']:
  raw=(N/row['log']).read_bytes();assert(len(raw),hashlib.sha256(raw).hexdigest())==(row['bytes'],row['sha256'])
 assert[x['profile']for x in launch['outcomes']]==['arm64','x64','x86','x86-laa']
 groups=[]
 for profile,row in zip(['arm64','x64','x86','x86-laa'],launch['outcomes']):
  assert row['profile']==profile and row['exit-code']==0 and row['executable-before']==row['executable-after']
  before=load(N/('native-continue-v3-'+profile+'-before.log'));assert before==load(N/('native-continue-v3-'+profile+'-after.log'))and before['sha256']==row['executable-before']['sha256']
  assert before['machine']=={'arm64':0xaa64,'x64':0x8664,'x86':0x14c,'x86-laa':0x14c}[profile]and before['large-address-aware']==(profile!='x86')
  text=(N/row['log']).read_text();assert text.count('complete cases=4')==1 and'unexpected-'not in text
  header=dict(x.split('=',1)for x in text.splitlines()[0].split());assert header=={'profile':profile,'pointer':str(4 if profile.startswith('x86')else 8),'context-bytes':str({'arm64':912,'x64':1232,'x86':716,'x86-laa':716}[profile])}
  resumes,checks,events,jumps=parse(text,'resume'),parse(text,'check'),parse(text,'event'),parse(text,'abandon')
  assert len(resumes)==len(jumps)==4 and len(checks)==8 and len(events)==7
  workers=parse(text,'worker');assert len(workers)==4 and all(x['wait']=='00000000'and x['exit']=='0'for x in workers)
  captures=parse(text,'capture');assert len(captures)==(4 if profile.startswith('x86')else 0)
  for capture in captures:assert capture['pc']==capture['expected-pc']and capture['sp']==capture['expected-sp']
  inside=parse(text,'inside-alert');assert len(inside)==1 and inside[0]['case']=='16'and inside[0]['status']=='00000000'and inside[0]['seen']=='1'
  for number in range(13,17):
   jump=next(x for x in jumps if int(x['case'])==number);resume=next(x for x in resumes if int(x['case'])==number)
   assert jump['test']==('1'if number in[14,16]else'0')and jump['seen']=='1'and int(jump['target-pc'],16)>0 and int(jump['target-sp'],16)>0
   assert resume['test']=='1'and resume['seen']==('2'if number==14 else'1')and resume['last']=='0000C0DE'
   got=[x for x in events if int(x['case'])==number];wanted=[(0x1111,1)]+([]if number==15 else[(0x2222,1 if number==14 else 3)])
   assert[(int(x['value'],16),int(x['phase']))for x in got]==wanted
   for index,x in enumerate(got):assert x['index']==str(index)and x['last']=='0000C0DE'and x['tid']==resume['tid']
   selected=[x for x in checks if int(x['case'])==number];assert[x['index']for x in selected]==['0','1']
   for index,x in enumerate(selected):assert x['status']=='00000000'and x['last']=='0000C0DE'and int(x['before'])==(int(resume['seen'])if index==0 else len(got))and int(x['after'])==len(got)
  groups.append({'profile':profile,'resumes':resumes,'checks':checks,'events':events,'jumps':jumps})
 result={'native-resumes':16,'native-post-checks':32,'native-callbacks':28,'groups':groups}
 assert load(D/'supplement-cases.json')==result
 return result
if __name__=='__main__':print(json.dumps({k:v for k,v in check().items()if k!='groups'},indent=2))
