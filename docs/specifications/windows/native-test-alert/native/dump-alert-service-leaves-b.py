from pathlib import Path
import hashlib,json,struct
S=Path(__file__).parent;record=S/'alert-service-leaf-observation-b.json';assert not record.exists()
def u32(raw,at):return struct.unpack_from('<I',raw,at)[0]
def u16(raw,at):return struct.unpack_from('<H',raw,at)[0]
rows=[]
for directory in ['System32','SysWOW64']:
 p=Path('C:/Windows')/directory/'ntdll.dll';raw=p.read_bytes();nt=u32(raw,0x3c);sections=u16(raw,nt+6);size=u16(raw,nt+20);optional=nt+24;pe64=u16(raw,optional)==0x20b;preferred=struct.unpack_from('<Q'if pe64 else'<I',raw,optional+(24 if pe64 else 28))[0];table=optional+size
 def offset(rva):
  for i in range(sections):
   start=table+i*40;va=u32(raw,start+12);length=max(u32(raw,start+8),u32(raw,start+16))
   if va<=rva<va+length:return u32(raw,start+20)+rva-va
  raise ValueError(hex(rva))
 export=offset(u32(raw,optional+(112 if pe64 else 96)));functions=offset(u32(raw,export+28));names=offset(u32(raw,export+32));ordinals=offset(u32(raw,export+36));wanted={}
 for index in range(u32(raw,export+24)):
  nameoff=offset(u32(raw,names+index*4));end=raw.index(0,nameoff);name=raw[nameoff:end].decode('ascii')
  if name in ['NtTestAlert','NtYieldExecution','Wow64Transition']:
   ordinal=u16(raw,ordinals+index*2);rva=u32(raw,functions+ordinal*4);at=offset(rva);wanted[name]={'rva':rva,'bytes64-hex':raw[at:at+64].hex()}
 assert 'NtTestAlert'in wanted
 if directory=='SysWOW64':
  code=bytes.fromhex(wanted['NtTestAlert']['bytes64-hex']);thunk=u32(code,6);at=offset(thunk-preferred);wanted['NtTestAlert']['transition-thunk']={'address':thunk,'rva':thunk-preferred,'bytes6-hex':raw[at:at+6].hex()}
 rows.append({'path':str(p),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'preferred-base':preferred,'machine':u16(raw,nt+4),'exports':wanted,'after-sha256':hashlib.sha256(p.read_bytes()).hexdigest()})
record.write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows),flush=True)
