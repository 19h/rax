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
for family,stem,count in [('probe-v5','native-thread-token-open-probe-v5',6588),('fresh','native-thread-token-open-fresh',2),('pseudo','native-thread-token-pseudo',40),('boundary','native-thread-token-boundary',24)]:
 for profile in (('arm64',)if family=='fresh'else('arm64','x64','x86','x86-laa')):
  directory=TASK/('thread-token-open-'+profile)
  source=directory/(stem+'.cpp');exe=directory/(stem+'-'+profile+'.exe');log=directory/(stem+'-'+profile+'.log');text=log.read_text();assert 'failure=' not in text and f'complete cases={count}' in text
  e=identity(exe,True);assert e['machine']=={'arm64':0xaa64,'x64':0x8664,'x86':0x14c,'x86-laa':0x14c}[profile]
  assert bool(e['characteristics']&0x20)==(profile!='x86')
  producers.append({'family':family,'profile':profile,'source':source.name,'log':log.name,'source-identity':identity(source),'executable-identity':e,'stdout-identity':identity(log)})
r={'captured-utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'kernel':list(sys.getwindowsversion()),'principal':subprocess.check_output(['whoami','/user','/fo','csv','/nh'],text=True).strip(),'python':sys.version,'inspector-identity':identity(Path(__file__)),'native-libraries':[identity(Path(r'C:\Windows\System32\ntdll.dll'),True),identity(Path(r'C:\Windows\SysWOW64\ntdll.dll'),True)],'producers':producers}
(SHARE/'native-thread-token-producer-identities.json').write_text(json.dumps(r,indent=2)+'\n');print('actual-source-exe-stdout-identities',len(producers))
