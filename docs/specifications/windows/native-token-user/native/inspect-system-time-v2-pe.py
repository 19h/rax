from pathlib import Path
import hashlib,json,struct,sys
p=Path(sys.argv[1]);raw=p.read_bytes();pe=struct.unpack_from('<I',raw,60)[0]
if raw[:2]!=b'MZ' or raw[pe:pe+4]!=b'PE\0\0':raise RuntimeError('actual PE headers missing')
machine=struct.unpack_from('<H',raw,pe+4)[0];flags=struct.unpack_from('<H',raw,pe+22)[0]
profile=sys.argv[2];expected={'arm64':0xaa64,'x64':0x8664,'x86':0x14c,'x86-laa':0x14c}[profile]
if machine!=expected or bool(flags&0x20)!=(profile!='x86'):raise RuntimeError('actual architecture/LAA mismatch')
print(json.dumps({'profile':profile,'path':str(p),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'machine':machine,'large-address-aware':bool(flags&0x20)}))
