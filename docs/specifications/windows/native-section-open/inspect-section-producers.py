from pathlib import Path
import datetime,hashlib,json,struct,subprocess,sys,ctypes
TASK=Path(r'C:\Windows\Temp\assist-native-rax-20261010')
SHARE=Path(r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154')
def require(ok,why):
 if not ok: raise ValueError(why)
def identity(p,pe=False):
 b=p.read_bytes();v={'path':str(p),'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()}
 if pe:
  require(b[:2]==b'MZ','DOS signature');at=struct.unpack_from('<I',b,0x3c)[0];require(b[at:at+4]==b'PE\0\0','PE signature');v.update(machine=struct.unpack_from('<H',b,at+4)[0],characteristics=struct.unpack_from('<H',b,at+22)[0])
 return v
producers=[]
for family,stem,count,profiles in [('fresh','native-section-open-fresh-v3',2,('arm64',)),('probe','native-section-open-probe-v3',5646,('arm64','x64','x86','x86-laa')),('duplicate','native-section-open-duplicate-v2',17760,('arm64','x64','x86','x86-laa'))]:
 for profile in profiles:
  directory=TASK/('section-open-'+profile)
  source=directory/(stem+'.cpp');exe=directory/(stem+'-'+profile+'.exe');log=directory/(stem+'-'+profile+'.log');text=log.read_text();require('failure=' not in text and f'complete cases={count}' in text,'complete capture')
  e=identity(exe,True);require(e['machine']=={'arm64':0xaa64,'x64':0x8664,'x86':0x14c,'x86-laa':0x14c}[profile],'producer machine');require(bool(e['characteristics']&0x20)==(profile!='x86'),'producer LAA')
  producers.append({'family':family,'profile':profile,'source':source.name,'log':log.name,'source-identity':identity(source),'executable-identity':e,'stdout-identity':identity(log)})
machine_out=ctypes.c_ushort();process_out=ctypes.c_ushort();query=ctypes.windll.kernel32.IsWow64Process2;query.argtypes=[ctypes.c_void_p,ctypes.POINTER(ctypes.c_ushort),ctypes.POINTER(ctypes.c_ushort)];require(query(ctypes.c_void_p(-1),ctypes.byref(process_out),ctypes.byref(machine_out))!=0,'native machine query');machine=machine_out.value
r={'native-machine':machine,'captured-utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'kernel':list(sys.getwindowsversion()),'principal':subprocess.check_output(['whoami','/user','/fo','csv','/nh'],text=True).strip(),'python':sys.version,'inspector-identity':identity(Path(__file__)),'native-libraries':[identity(Path(r'C:\Windows\System32\ntdll.dll'),True),identity(Path(r'C:\Windows\SysWOW64\ntdll.dll'),True)],'producers':producers}
(SHARE/'native-section-producer-identities.json').write_text(json.dumps(r,indent=2)+'\n');print('actual-source-exe-stdout-identities',len(producers))
