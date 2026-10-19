from pathlib import Path
import hashlib,json,shutil
N=Path(__file__).resolve().parent;R=N/'wait-source';S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
assert not (N/'wait-owning-source-hashes-epoch4.json').exists()
m=json.loads((N/'wait-owning-source-hashes-b.json').read_text());paths={r['path'] for r in m['sources']};paths.add('src/user/windows/dll/mod.rs')
m['sources']=[]
for name in sorted(paths):
 raw=(R/name).read_bytes();m['sources'].append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
m['epoch']='native-wait-epoch4'
helper=(N/'wait-owned-cpp-build-b.py').read_text().replace('native-wait-final-owning-archive-identities.json','native-wait-epoch4-pre-cpp-archives.json')
name='wait-owned-cpp-build-epoch4.py';raw=helper.encode();m['helpers'][0]={'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'native-path':'C:/Windows/Temp/assist-native-rax-20261010/'+name}
for root in [N,S]:
 (root/name).write_text(helper);(root/'wait-owning-source-hashes-epoch4.json').write_text(json.dumps(m,indent=2)+'\n')
collector=(N/'collect-wait-archives.py').read_text().replace('native-wait-epoch3','native-wait-epoch4')
for root in [N,S]:(root/'collect-wait-archives-epoch4.py').write_text(collector)
for host in ['macos','linux']:
 text=(N/(host+'-wait-owning-commands.json')).read_text().replace('collect-wait-archives.py','collect-wait-archives-epoch4.py').replace('wait-owning-source-hashes.json','wait-owning-source-hashes-epoch4.json')
 text=text.replace(host+'-wait-owning-',host+'-wait-epoch4-owning-').replace(host+'-wait-final-owning-',host+'-wait-epoch4-final-owning-')
 (N/(host+'-wait-epoch4-owning-commands.json')).write_text(text)
runner=(N/'run-wait-owning-gates.py').read_text().replace("host+'-wait-owning-", "host+'-wait-epoch4-owning-")
(N/'run-wait-owning-epoch4.py').write_text(runner)
print('Prepared epoch4 consumer source identities',len(m['sources']))
