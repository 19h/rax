from pathlib import Path
import hashlib,json,struct
base=Path(r'C:\Windows\Temp\assist-native-rax-20261010')
rows=[]
for arch in ('arm64','x64','x86','x86-laa'):
    directory=base/('io-completion-probe-'+arch)
    binary=directory/('native-io-completion-create-probe-'+arch+'.exe')
    data=binary.read_bytes()
    pe=struct.unpack_from('<I',data,0x3c)[0]
    if data[pe:pe+4]!=b'PE\0\0':raise SystemExit('invalid PE')
    machine=struct.unpack_from('<H',data,pe+4)[0]
    flags=struct.unpack_from('<H',data,pe+22)[0]
    expected=0xaa64 if arch=='arm64' else 0x8664 if arch=='x64' else 0x14c
    if machine!=expected:raise SystemExit('wrong machine')
    if arch.startswith('x86') and bool(flags&0x20)!=(arch=='x86-laa'):raise SystemExit('wrong LAA')
    row={'profile':arch,'pe-machine':hex(machine),'large-address-aware':bool(flags&0x20),'files':[]}
    for path in (binary,directory/'native-io-completion-create-probe.cpp',directory/('native-io-completion-create-probe-'+arch+'.log')):
        content=path.read_bytes();row['files'].append({'path':str(path),'bytes':len(content),'sha256':hashlib.sha256(content).hexdigest()})
    rows.append(row)
out=Path(r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154')/'io-completion-producer-identities.json'
out.write_text(json.dumps(rows,indent=2)+'\n')
print(json.dumps(rows))
