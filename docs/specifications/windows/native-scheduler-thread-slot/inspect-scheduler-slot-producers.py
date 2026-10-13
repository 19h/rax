from pathlib import Path
import datetime,hashlib,json,struct,subprocess,sys
TASK=Path(r'C:\Windows\Temp\assist-native-rax-20261010')
SHARE=Path(r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154')
def identity(p,pe=False):
 b=p.read_bytes();v={'path':str(p),'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()}
 if pe:
  assert b[:2]==b'MZ';at=struct.unpack_from('<I',b,0x3c)[0];assert b[at:at+4]==b'PE\0\0';v.update(machine=struct.unpack_from('<H',b,at+4)[0],characteristics=struct.unpack_from('<H',b,at+22)[0])
 return v
producers=[]
for family in ('probe-v2','fresh-v2','sequence','resources-v2','pool','capacity','boundaries','boundaries-v2'):
 for profile in (('arm64',)if family in ('fresh-v2','sequence')else('arm64','x64','x86','x86-laa')):
  directory=TASK/('scheduler-thread-slot-'+profile);stem='native-scheduler-thread-slot-'+family
  source=directory/(stem+'.cpp');exe=directory/(stem+'-'+profile+'.exe');log=directory/(stem+'-'+profile+'.log');text=log.read_text();assert 'failure=' not in text
  assert ('complete cases=9872'if family=='probe-v2'else'complete cases=2 warmup=1'if family in ('fresh-v2','sequence')else'complete rounds=3 warmup-excluded=1')in text
  e=identity(exe,True);assert e['machine']=={'arm64':0xaa64,'x64':0x8664,'x86':0x14c,'x86-laa':0x14c}[profile]
  assert bool(e['characteristics']&0x20)==(profile!='x86')
  producers.append({'family':family,'profile':profile,'source':source.name,'log':log.name,'source-identity':identity(source),'executable-identity':e,'stdout-identity':identity(log)})
r={'captured-utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'kernel':list(sys.getwindowsversion()),'principal':subprocess.check_output(['whoami','/user','/fo','csv','/nh'],text=True).strip(),'python':sys.version,'inspector-identity':identity(Path(__file__)),'native-libraries':[identity(Path(r'C:\Windows\System32\ntdll.dll'),True),identity(Path(r'C:\Windows\SysWOW64\ntdll.dll'),True)],'producers':producers}
(SHARE/'native-scheduler-slot-producer-identities.json').write_text(json.dumps(r,indent=2)+'\n');print('actual-source-exe-stdout-identities',len(producers))
