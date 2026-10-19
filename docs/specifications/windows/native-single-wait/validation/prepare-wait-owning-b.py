from pathlib import Path
import hashlib,json

N=Path(__file__).resolve().parent
S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
D=N/'owning-windows-wait-b';D.mkdir(exist_ok=True)
source=(S/'shared-section-owned-cpp-build.py').read_text().replace('target-io-completion-owned','target-wait-owned')
source=source.replace('import subprocess','import subprocess,hashlib,json')
needle="def run(args):"
added="""def identity(path):return {'path':str(path),'bytes':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
transfer=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
metadata=json.loads((transfer/'native-wait-final-owning-archive-identities.json').read_text())
selected=next(row for row in metadata['archives'] if row['path'].endswith('.lib'))
assert Path(selected['path'])==archive,'C++ helper did not select the actual Cargo carrier'
before=identity(archive)
assert before['sha256']==selected['sha256'] and before['bytes']==selected['bytes'],'actual carrier changed before C++ link'
print('linked-archive-before '+json.dumps(before),flush=True)
"""
assert source.count(needle)==1;source=source.replace(needle,added+needle)
source += "\nafter=identity(archive)\nassert after==before,'carrier changed during C++ validation'\nprint('linked-archive-after '+json.dumps(after),flush=True)\n"
name='wait-owned-cpp-build-b.py'
for root in [N,S,D]:(root/name).write_text(source)
raw=source.encode();m=json.loads((N/'wait-owning-source-hashes.json').read_text())
m['helpers'][0]={'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'native-path':'C:/Windows/Temp/assist-native-rax-20261010/'+name}
for root in [N,S]:(root/'wait-owning-source-hashes-b.json').write_text(json.dumps(m,indent=2)+'\n')
for name in ['verify-wait-owning-sources.py','inspect-wait-final-archives.py','assist-native-wait-final-owned.ps1','run-wait-final-owned.py']:
    text=(N/'owning-windows-wait'/name).read_text()
    text=text.replace('verify-wait-owning-sources.py','verify-wait-owning-sources-b.py')
    text=text.replace('inspect-wait-final-archives.py','inspect-wait-final-archives-b.py')
    text=text.replace('assist-native-wait-final-owned.ps1','assist-native-wait-final-owned-b.ps1')
    text=text.replace('wait-owning-source-hashes.json','wait-owning-source-hashes-b.json')
    text=text.replace('shared-section-owned-cpp-build.py','wait-owned-cpp-build-b.py')
    text=text.replace('native-wait-owning-source-identities.json','native-wait-owning-source-identities-b.json')
    text=text.replace('native-wait-final-owning-archive-identities.json','native-wait-final-owning-archive-identities-b.json')
    text=text.replace('native-wait-final-owning-archive-hashes.json','native-wait-final-owning-archive-hashes-b.json')
    text=text.replace('native-wait-final-after-artifacts','native-wait-final-b-after-artifacts')
    text=text.replace('assist-native-wait-final-shipping','assist-native-wait-final-b-shipping')
    text=text.replace('assist-native-wait-final-cpp','assist-native-wait-final-b-cpp')
    text=text.replace('native-wait-final-owning-source-verification','native-wait-final-b-owning-source-verification')
    text=text.replace('wait-final-owned-launch','wait-final-owned-b-launch').replace('wait-final-owned.lock','wait-final-owned-b.lock').replace('wait-final-owned-driver','wait-final-owned-b-driver')
    if name=='run-wait-final-owned.py':
        text=text.replace("if record.exists():", "prior=json.loads((s/'wait-final-owned-launch.json').read_text())\nif prior['state']!='finished' or prior['exit-code']!=0:raise RuntimeError('prior retained-owner run must finish before corrected C++ replay')\nif record.exists():")
    stem,suffix=name.rsplit('.',1);newname=stem+'-b.'+suffix
    for root in [D,S]:(root/newname).write_text(text)
# Actual ordinary observations must use the final corrected owning run.
p=N/'run-wait-owner-observer.py';text=p.read_text().replace('wait-final-owned-launch','wait-final-owned-b-launch').replace('native-wait-final-owning-archive-identities','native-wait-final-owning-archive-identities-b')
for root in [N,S]:(root/p.name).write_text(text)
p=N/'wait-owner-observer-build.ps1';text=p.read_text().replace('native-wait-final-owning-archive-identities','native-wait-final-owning-archive-identities-b')
for root in [N,S]:(root/p.name).write_text(text)
print('Prepared independently named native consumer replay with actual-carrier before/after binding')
