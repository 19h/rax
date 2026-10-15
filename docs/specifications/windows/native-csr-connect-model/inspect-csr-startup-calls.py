from pathlib import Path
import hashlib,json,struct,re,sys,datetime
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');p=Path('C:/Windows/Temp/assist-native-rax-20261010/csr-startup-calls-arm64')
def require(x,m):
 if not x:raise ValueError(m)
def identity(path,pe=False):
 b=path.read_bytes();j={'path':str(path),'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()}
 if pe:
  require(b[:2]==b'MZ','DOS signature');off=struct.unpack_from('<I',b,0x3c)[0];require(b[off:off+4]==b'PE\0\0','PE signature');j['machine']=struct.unpack_from('<H',b,off+4)[0];j['characteristics']=struct.unpack_from('<H',b,off+22)[0]
 return j
require(__debug__,'optimized verification rejected');outcome=json.loads((s/'native-csr-startup-calls-outcomes.json').read_text(encoding='utf-8-sig'));src=identity(p/'native-csr-startup-calls.cpp');exe=identity(p/'native-csr-startup-calls-arm64.exe',True);stdout=identity(p/'native-csr-startup-calls-arm64.log')
require(src['sha256']=='d19ec5a559ee76e0ba02f6d934a223937eb7bff9099bc4dfe5b773ade95d2f8b'==outcome['source-hash'].lower(),'actual source');require(exe['machine']==0xaa64 and exe['sha256']==outcome['exe-sha256'].lower() and exe['bytes']==outcome['exe-bytes'] and outcome['exit-code']==0,'actual ARM64 PE outcome');require(stdout['sha256']==identity(s/'native-csr-startup-calls-arm64.log')['sha256'],'actual stdout transfer')
text=(p/'native-csr-startup-calls-arm64.log').read_text();entries=re.findall(r'^entry ordinal=(\d+) name=(\w+) caller=[A-F0-9]+ args=([A-F0-9,]+)$',text,re.M);require(len(entries)==1 and entries[0][1]=='NtSecureConnectPort' and len(entries[0][2].rstrip(',').split(','))==9,'one original9argument secure call');require('return ordinal=0 name=NtSecureConnectPort status=00000000' in text and text.splitlines()[-1]=='complete installed=1 entered=1 returned=1 exit=41','successful unmodified connection and natural child41');require('hook name=NtAlpcConnectPort' in text and 'hook name=NtAlpcSendWaitReceivePort' in text and 'hook name=NtRequestWaitReplyPort' in text,'all monitored entry points installed')
record={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'kernel':list(sys.getwindowsversion()),'source':src,'executable':exe,'stdout':stdout,'inspector':identity(Path(__file__)),'entered':1,'returned':1,'native-child-exit':41,'ordinary-emulated-exits':0,'scope':'four monitored main-thread native entry points; absence is bounded to those entry points and this natural owned child startup'}
(s/'native-csr-startup-calls-identities.json').write_text(json.dumps(record,indent=2)+'\n');print('Verified actual native CSR observer source,ARM64 PE,stdout,one9argument call and natural child41',flush=True)
