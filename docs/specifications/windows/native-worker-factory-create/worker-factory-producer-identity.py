import pathlib,struct,json,hashlib
root=pathlib.Path(r'C:\Windows\Temp\assist-native-rax-20261010')
rows=[]
for profile in ('arm64','x64','x86','x86-laa'):
 file=root/('worker-factory-basic-probe-'+profile)/('native-worker-factory-basic-probe-'+profile+'.exe')
 data=file.read_bytes();nt=struct.unpack_from('<I',data,0x3c)[0];machine=struct.unpack_from('<H',data,nt+4)[0];flags=struct.unpack_from('<H',data,nt+22)[0];opt=nt+24;magic=struct.unpack_from('<H',data,opt)[0];width=8 if magic==0x20b else 4;off=88 if width==8 else 72;reserve,commit=struct.unpack_from('<QQ' if width==8 else '<II',data,opt+off)
 rows.append({'profile':profile,'path':str(file),'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'machine':hex(machine),'LAA':bool(flags&0x20),'PE-stack-reserve':reserve,'PE-stack-commit':commit})
print(json.dumps(rows,indent=2))
