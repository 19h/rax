from pathlib import Path
import hashlib,json,os,shutil
Q=Path(__file__).parent;R=Q/'registry-source';O=Q.parent/'native-semaphore-20261019';S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
def rewrite(text):
 return text.replace(str(O/'semaphore-integrated-source'),str(R)).replace(str(O),str(Q)).replace('semaphore-epoch2','registry-epoch1').replace('semaphore-owning-source-hashes-epoch2','registry-owning-source-hashes-epoch1').replace('semaphore-owned-cpp-build-epoch2','registry-owned-cpp-build-epoch1').replace('collect-semaphore-archives-epoch2','collect-registry-archives-epoch1').replace('owning-linux-cpp-build-epoch2','owning-linux-cpp-build-registry-epoch1')
for old,new in [('collect-semaphore-archives-epoch2.py','collect-registry-archives-epoch1.py'),('owning-linux-cpp-build-epoch2.py','owning-linux-cpp-build-registry-epoch1.py'),('semaphore-owned-cpp-build-epoch2.py','registry-owned-cpp-build-epoch1.py'),('run-semaphore-owning-epoch2.py','run-registry-owning-epoch1.py')]:
 s=rewrite((O/old).read_text()).replace("R = O / 'semaphore-integrated-source'", "R = O / 'registry-source'")
 if 'cpp-build' in new and 'linux' not in new:s=s.replace('assist-native-rax-20261010/src/tests/fixtures','assist-native-rax-20261010/src-registry-epoch1/tests/fixtures')
 (Q/new).write_text(s)
for host in ['macos','linux']:
 s=rewrite((O/(host+'-semaphore-epoch2-owning-commands.json')).read_text())
 (Q/(host+'-registry-epoch1-owning-commands.json')).write_text(s)
x=json.loads((O/'semaphore-owning-source-hashes-epoch2.json').read_text())
names={row['path'] for row in x['sources']}
names.update(p for p in json.loads((Q/'registry-source-ownership.json').read_text())['source-owned-paths'] if p.endswith('.rs'))
x['sources']=[]
for name in sorted(names):
 raw=(R/name).read_bytes();x['sources'].append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
raw=(Q/'registry-owned-cpp-build-epoch1.py').read_bytes()
x['helpers'][0]={'path':'registry-owned-cpp-build-epoch1.py','bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'native-path':'C:/Windows/Temp/assist-native-rax-20261010/registry-owned-cpp-build-epoch1.py'}
x.update(feature='native-state-separation-registry',epoch=1,**{'source-parent-rax':'daecebd58bd574fd95281ff2635b0754f6e1dede'})
x['scope']='Selected inputs with hashes and separately bound helpers; not the complete Cargo dependency closure. Full source epoch1 manifest binds all repository inputs. Retained Assist Rust ABI184 and exact selected C++ consumers, not whole current root products.'
(Q/'registry-owning-source-hashes-epoch1.json').write_text(json.dumps(x,indent=2)+'\n')
s=(O/'run-semaphore-native-owning-epoch2.py').read_text()
s=s.replace("R=B/'src-semaphore-epoch2'", "R=B/'src-registry-epoch1'")
s=s.replace('semaphore-native-owning-epoch2','registry-native-owning-epoch1').replace('semaphore-native-epoch2-gates','registry-native-epoch1-gates').replace('semaphore-owning-source-hashes-epoch2','registry-owning-source-hashes-epoch1').replace('semaphore-owned-cpp-build-epoch2','registry-owned-cpp-build-epoch1').replace('collect-semaphore-archives-epoch2','collect-registry-archives-epoch1').replace('native-semaphore-epoch2','native-registry-epoch1').replace("'source-epoch':2", "'source-epoch':1")
s=s.replace("old=B/'src-semaphore-epoch1'", "old=B/'src-semaphore-epoch2'").replace("load(S/'semaphore-native-owning-epoch1.json')", "load(S/'semaphore-native-owning-epoch2.json')").replace('selected506 inputs','selected'+str(len(x['sources']))+' inputs')
(Q/'run-registry-native-owning-epoch1.py').write_text(s)
for name in ['registry-owning-source-hashes-epoch1.json','registry-owned-cpp-build-epoch1.py','collect-registry-archives-epoch1.py','run-registry-native-owning-epoch1.py']:
 shutil.copy2(Q/name,S/name)
print('selected owning inputs',len(x['sources']))
