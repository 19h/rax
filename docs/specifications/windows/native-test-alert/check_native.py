"""Independent native alert/APC control and identity checker; no product imports."""
from pathlib import Path
import hashlib,json,re,sys
D=Path(__file__).resolve().parent;N=D/'native';PROFILES=['arm64','x64','x86','x86-laa'];SHA='6393a325c24f1b1c93bed00b05d8d4ecb54fab80693962f3bc278560f0fe9172'
def load(path):
 raw=path.read_bytes();return json.loads(raw.decode('utf-16')if raw[:2]in[b'\xff\xfe',b'\xfe\xff']else raw.decode('utf-8-sig'))
def parse(text,prefix):return [dict(x.split('=',1)for x in line.split()[1:])for line in text.splitlines()if line.startswith(prefix+' ')]
def check():
 manifest=load(N/'manifest.json');names=set()
 for x in manifest['files']:
  assert x['path']not in names;names.add(x['path']);raw=(N/x['path']).read_bytes();assert len(raw)==x['bytes']and hashlib.sha256(raw).hexdigest()==x['sha256'],x['path']
 assert {x.name for x in N.iterdir()if x.is_file()}==names|{'manifest.json'}
 assert hashlib.sha256((N/'native-test-alert-v1.cpp').read_bytes()).hexdigest()==SHA
 primary=load(D/'primary/sources.json')
 for x in primary:
  raw=(D/'primary'/x['file']).read_bytes();assert len(raw)==x['bytes']and hashlib.sha256(raw).hexdigest()==x['sha256']
 groups=[];runtime=None
 for version in ['v1','v1b']:
  launch=load(N/f'native-test-alert-{version}-launch.json');assert launch['state']=='finished'and launch['exit-code']==0
  assert launch['source-before']==launch['source-after']and launch['source-before']['sha256']==SHA
  if version=='v1':
   modules=load(N/'native-test-alert-v1-modules-before.json');assert modules==load(N/'native-test-alert-v1-modules-after.json');hashes=[x['Hash'].lower()for x in modules];outcomes=load(N/'native-test-alert-v1-outcomes.json')
  else:
   assert launch['runtime-modules-before']==launch['runtime-modules-after'];hashes=[x['sha256']for x in launch['runtime-modules-before']];outcomes=launch['outcomes']
   assert len(launch['steps'])==16 and all(x['exit-code']==0 for x in launch['steps'])
   for x in launch['steps']:
    raw=(N/x['log']).read_bytes();assert len(raw)==x['bytes']and hashlib.sha256(raw).hexdigest()==x['sha256']
  assert len(hashes)==6 and hashes[0]=='502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f'and hashes[1]=='ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e'
  assert runtime is None or runtime==hashes;runtime=hashes
  assert [x['profile']for x in outcomes]==PROFILES and all(x['exit-code']==0 for x in outcomes)
  for profile,outcome in zip(PROFILES,outcomes):
   ext='json'if version=='v1'else'log';prefix=f'native-test-alert-{version}-{profile}'
   before=load(N/(prefix+'-before.'+ext));after=load(N/(prefix+'-after.'+ext));assert before==after
   assert before['machine']=={'arm64':0xAA64,'x64':0x8664,'x86':0x14C,'x86-laa':0x14C}[profile]
   assert before['large-address-aware']==(profile!='x86')
   if version=='v1':assert load(N/(prefix+'-source-after.json'))['Hash'].lower()==SHA
   else:assert outcome['executable-before']==outcome['executable-after'] and outcome['executable-before']['sha256']==before['sha256']
   text=(N/(prefix+'.log')).read_text();assert text.count('complete cases=13 checks=27')==1
   header=dict(x.split('=',1)for x in text.splitlines()[0].split());assert header['profile']==profile and int(header['pointer-bytes'])==(8 if profile in ['arm64','x64']else 4)
   assert all(int(header[x],16)>0 for x in ['test-alert','alert-thread','queue-apc'])
   checks=parse(text,'check');events=parse(text,'event');alerts=parse(text,'alert');queues=parse(text,'queue')
   assert len(checks)==27 and len(events)==18 and len(alerts)==4 and len(queues)==17,(version,profile,len(checks),len(events),len(queues))
   for row in queues:assert int(row['result'],16)==(1 if row['kind']=='1'else 0)and row['last']=='0000C0DE'and row['seen']=='0'
   assert {int(x['case'])for x in alerts}=={1,2,10,11}
   assert all(x['status']=='00000000'and x['last']=='0000C0DE'and x['seen']=='0'for x in alerts)
   for number in range(13):
    selected=[x for x in checks if int(x['case'])==number];assert [int(x['call'])for x in selected]==list(range(3 if number==2 else 2))
    wanted={0:[],1:[],2:[],3:[(1,0x1111,4)],4:[(1,0x1111,4),(1,0x2222,4)],5:[(1,0x1111,4),(1,0x2222,4),(1,0x3333,4)],6:[(1,0x1111,4),(1,0x2222,6)],7:[(3,0x5555,4)],8:[(3,0x5555,4),(3,0x6666,4),(3,0x7777,4)],9:[(3,0x5555,4),(1,0x1111,4),(3,0x6666,4)],10:[(3,0x5555,5)],11:[(1,0x1111,5)],12:[(1,0x4444,4)]}[number]
    got=[x for x in events if int(x['case'])==number]
    assert [(int(x['kind']),int(x['a'],16),int(x['phase']))for x in got]==wanted
    assert [int(x['index'])for x in got]==list(range(len(wanted)))
    for x in got:
     a=int(x['a'],16);assert int(x['b'],16)==(a+1 if x['kind']=='3'else 0)and int(x['c'],16)==(a+2 if x['kind']=='3'else 0)
     assert x['last']=='0000C0DE'and x['nested']=='00000000'
    for call,x in enumerate(selected):
     first_alert=call==0 and number in [1,2,10,11];expected_status=0x101 if first_alert else 0
     count=len(wanted)if not(first_alert or number in [1,2])else 0
     assert int(x['status'],16)==expected_status and x['last']==('0000ABCD'if number==12 and call==0 else'0000C0DE')
     assert x['nonvolatile']=='1'and x['sp-before']==x['sp-after']
     assert int(x['before'])==(0 if call==0 or number in [10,11]and call==1 else len(wanted))
     assert int(x['after'])==count
     assert all(e['tid']==x['tid']for e in got)
   groups.append({'version':version,'profile':profile,'checks':checks,'events':events,'queues':queues,'alerts':alerts})
 observed=load(N/'alert-service-leaf-observation-b.json');assert len(observed)==2
 for x in observed:assert x['sha256']==x['after-sha256']
 assert observed[0]['sha256']=='502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f'
 x=observed[1];assert x['sha256']=='ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e'and x['machine']==0x14c
 leaf=x['exports']['NtTestAlert'];code=bytes.fromhex(leaf['bytes64-hex']);assert code[0]==0xb8 and int.from_bytes(code[1:5],'little')==0x000201d3 and code[5]==0xba and code[10:13]==bytes([0xff,0xd2,0xc3])
 thunk=leaf['transition-thunk'];body=bytes.fromhex(thunk['bytes6-hex']);assert int.from_bytes(code[6:10],'little')==thunk['address']==x['preferred-base']+thunk['rva']and body[:2]==bytes([0xff,0x25])and int.from_bytes(body[2:],'little')==x['preferred-base']+x['exports']['Wow64Transition']['rva']
 result={'native-checks':sum(len(x['checks'])for x in groups),'native-callbacks':sum(len(x['events'])for x in groups),'groups':groups,'kernel-scope':'one Windows29683 ARM64 kernel; four ABI views, two independent build/launch paths'}
 assert load(D/'cases.json')==result
 modeled=load(D/'modeled-cases.json');allowed={0,1,2,3,4,5,6,11,12}
 projection=[{'version':x['version'],'profile':x['profile'],**{key:[row for row in x[key]if int(row['case'])in allowed]for key in ['checks','events']}}for x in groups]
 assert modeled['groups']==projection
 assert modeled['native-checks']==sum(len(x['checks'])for x in projection)==152
 assert modeled['native-callbacks']==sum(len(x['events'])for x in projection)==80
 assert modeled['native-control-checks']==result['native-checks']-modeled['native-checks']==64
 return result
if __name__=='__main__':
 result=check();print(json.dumps({k:v for k,v in result.items()if k!='groups'},indent=2))
