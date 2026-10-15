from pathlib import Path
import hashlib,json,re,struct,datetime,sys
TASK=Path("C:/Windows/Temp/assist-native-rax-20261010")
SHARE=Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")

def require(ok,message):
 if not ok:raise ValueError(message)

def identity(path,pe=False):
 b=path.read_bytes();row={"path":str(path),"bytes":len(b),"sha256":hashlib.sha256(b).hexdigest()}
 if pe:
  require(b[:2]==b"MZ","DOS signature");at=struct.unpack_from("<I",b,60)[0];require(b[at:at+4]==b"PE\0\0","PE signature");row["machine"]=struct.unpack_from("<H",b,at+4)[0]
 return row

def captured(text,name):
 matches=re.findall(r"^dump name="+name+r" .*requested=(\d+) ok=1 error=0 got=(\d+) hex=([0-9A-F]*)$",text,re.M);require(len(matches)==1,"one complete "+name);requested,got,hexdata=matches[0];b=bytes.fromhex(hexdata);require(int(requested)==int(got)==len(b),"extent "+name);return b

require(__debug__,"optimized validation rejected")
rows=[]
for kind,expected in [("csr-readonly-metadata","e7b4a8fa4c9d9c5fe05d048223e203ffd0719df31c627dc67d2a977062405f4b"),("csr-readonly-metadata2","e5de1967ccd3e0596453f923125ab3cca137f34c9a61f774a969716cf01461d0")]:
 d=TASK/(kind+"-arm64");stem="native-"+kind+"-arm64";source=identity(d/("native-"+kind+".cpp"));exe=identity(d/(stem+".exe"),True);output=identity(d/(stem+".log"));outcome=json.loads((SHARE/("native-"+kind+"-outcomes.json")).read_text(encoding="utf-8-sig"));require(source["sha256"]==expected==outcome["source-hash"].lower(),"actual compiled source");require(exe["machine"]==0xaa64 and exe["sha256"]==outcome["exe-sha256"].lower() and exe["bytes"]==outcome["exe-bytes"] and outcome["exit-code"]==0,"actual ARM64 executable");require(output["sha256"]==identity(SHARE/(stem+".log"))["sha256"],"raw transfer")
 text=(d/(stem+".log")).read_text();require(text.splitlines()[-1]=="complete status=0 read-only=1","complete readonly control")
 profile=re.search(r"^profile width=8 pid=(\d+) ntdll=([0-9A-F]+) peb=([0-9A-F]+)$",text,re.M);require(profile is not None,"native profile");peb=captured(text,"peb");prefix=captured(text,"selected-prefix");code=captured(text,"csr-pid-code");require(len(peb)==2048 and len(prefix)==128 and len(code)==64,"bounded actual reads")
 shared=int.from_bytes(peb[0x88:0x90],"little");statics=int.from_bytes(peb[0x98:0xa0],"little");server=int.from_bytes(prefix[40:48],"little");offsets=[i for i in range(0,len(peb)-8,8) if int.from_bytes(peb[i:i+8],"little")==server];require(statics-shared==0x750 and server!=shared and offsets==[0x380],"selected native server/client address relationship")
 pid=re.findall(r"^csr-pid-value value=([0-9A-F]+)$",text,re.M)
 row={"kind":kind,"source":source,"executable":exe,"stdout":output,"client-shared-base":shared,"client-statics-base":statics,"static-offset":statics-shared,"server-shared-base":server,"server-base-peb-offset":offsets[0],"getter-code-sha256":hashlib.sha256(code).hexdigest()}
 if kind.endswith("2"):
  require(len(pid)==1,"actual pointer-width getter");row["server-pid"]=int(pid[0],16);require(row["server-pid"]!=int(profile[1]) and row["server-pid"]>0,"distinct server identity")
 rows.append(row)
require(rows[0]["getter-code-sha256"]==rows[1]["getter-code-sha256"] and rows[0]["server-shared-base"]==rows[1]["server-shared-base"],"stable selected runtime acquisition")
raw=(SHARE/"native-secure-connect-first-call2-arm64.log").read_text();response=captured(raw,"connection-after");require(int.from_bytes(response[:8],"little")==rows[1]["server-shared-base"] and int.from_bytes(response[8:16],"little")-rows[1]["server-shared-base"]==rows[1]["static-offset"] and int.from_bytes(response[16:24],"little")==rows[1]["server-pid"],"independent original CSR response matches fixed readonly acquisition")
record={"utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),"kernel":list(sys.getwindowsversion()),"producers":rows,"inspector":identity(Path(__file__)),"scope":"actual ARM64 kernel29683 selected runtime only; captured addresses are evidence,not constants; no emulated success"};(SHARE/"native-csr-metadata-identities.json").write_text(json.dumps(record,indent=2)+"\n");print("Verified two actual ARM64 readonly metadata producers and independent original CSR connection response relationships")
