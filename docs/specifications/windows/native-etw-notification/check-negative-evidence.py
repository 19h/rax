from pathlib import Path
import json,hashlib,shutil,subprocess
b=Path(__file__).parent;r=Path('/Users/int/hexrays/kvasir/vendor/rax');e=r/'docs/specifications/windows/native-etw-notification'
parent=b/'negative-cases-v3';parent.mkdir(exist_ok=True)
results=[]
def rehash(p,name):
    m=json.loads((p/'evidence-manifest.json').read_text());f=p/name;m[name]=dict(bytes=f.stat().st_size,sha256=hashlib.sha256(f.read_bytes()).hexdigest());(p/'evidence-manifest.json').write_text(json.dumps(m,indent=2)+'\n')
for case in ['identity','missing-entry','duplicate-row','rehashed-status','producer-machine','compiled-source']:
    p=parent/case;shutil.copytree(e,p);source=r
    if case=='identity':
        f=p/'phnt-ntwmi.h';f.write_bytes(f.read_bytes()+b' ')
    elif case=='missing-entry':
        f=p/'evidence-manifest.json';m=json.loads(f.read_text());del m['phnt-ntwmi.h'];f.write_text(json.dumps(m,indent=2)+'\n')
    elif case in ['duplicate-row','rehashed-status']:
        name='native-etw-notification-event-arm64.log';f=p/name;lines=f.read_text().splitlines()
        if case=='duplicate-row':lines[1]=lines[0]
        else:lines[4]=lines[4].replace('status=C0000718','status=00000000')
        f.write_text('\n'.join(lines)+'\n');rehash(p,name)
        identity_name='native-etw-notification-producer-identities.json';identity_file=p/identity_name;identity=json.loads(identity_file.read_text());producer=next(r for r in identity['producers'] if r['log']==name);producer['log_identity']['bytes']=f.stat().st_size;producer['log_identity']['sha256']=hashlib.sha256(f.read_bytes()).hexdigest();identity_file.write_text(json.dumps(identity,indent=2)+'\n');rehash(p,identity_name)
    elif case=='producer-machine':
        name='native-etw-notification-producer-identities.json';f=p/name;m=json.loads(f.read_text());m['producers'][0]['executable_identity']['machine']=0x14C;f.write_text(json.dumps(m,indent=2)+'\n');rehash(p,name)
    else:
        source=parent/'altered-source';source.mkdir()
        for row in json.loads((p/'owning-source-hashes.json').read_text())['sources']:
            f=source/row['path'];f.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(r/row['path'],f)
        f=source/'src/user/windows/dll/native/trace.rs';f.write_bytes(f.read_bytes()+b'\n// controlled mismatch\n')
    command=['python3',str(p/'check-evidence.py'),str(p),str(source)]
    output=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    (b/('negative-'+case+'.log')).write_text(output.stdout)
    if output.returncode==0:raise RuntimeError('corruption accepted '+case)
    reason={'identity':'evidence identity phnt-ntwmi.h','missing-entry':'exact evidence coverage','duplicate-row':'exact mature handle partitions','rehashed-status':'typed access length status','producer-machine':'actual PE architecture','compiled-source':'current compiled source src/user/windows/dll/native/trace.rs'}[case]
    if reason not in output.stdout.splitlines()[-1]:raise RuntimeError('unexpected rejection for '+case+': '+output.stdout.splitlines()[-1])
    results.append(dict(case=case,command=command,**{'exit-code':output.returncode,'rejected':True,'reason':output.stdout.splitlines()[-1]}))
(b/'negative-validation.json').write_text(json.dumps(results,indent=2)+'\n')
print('six controlled corruptions rejected')
