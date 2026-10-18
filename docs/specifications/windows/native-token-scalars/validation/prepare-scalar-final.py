from pathlib import Path
import datetime,hashlib,json,os,re,subprocess
B=Path(__file__).resolve().parent;R=B/'rax-source';U=B.parent/'native-token-session-id-20261017';S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');M=Path('/Users/int/hexrays/kvasir')
def load(p):
 raw=p.read_bytes();return json.loads(raw.decode('utf-16')if raw[:2]in[b'\xff\xfe',b'\xfe\xff']else raw.decode('utf-8-sig'))
for h in ['macos','linux']:
 if load(B/(h+'-scalar-epoch2-focus.json'))['exit-code']!=0:raise RuntimeError('corrected focus failed '+h)
state=load(S/'scalar-epoch2-launch.json')
if state['state']=='finished':
 if state['exit-code']!=0:raise RuntimeError('corrected native focus failed')
elif state['state']=='running':
 if 'Verified scalar epoch2 source inputs: 5718' not in (S/'scalar-epoch2-driver.log').read_text():raise RuntimeError('native corrected source verification not reached')
 (B/'native-focus-pending-at-final-preparation.json').write_text(json.dumps(state,indent=2)+'\n')
else:raise RuntimeError('unknown native focus state')
manifest=load(B/'scalar-epoch2-source.json')
for row in manifest['files']:
 raw=(R/row['path']).read_bytes()
 if(len(raw),hashlib.sha256(raw).hexdigest())!=(row['bytes'],row['sha256']):raise RuntimeError('source drift '+row['path'])
for p in [B/'scalar-final-source.json',S/'scalar-final-source.json']:p.write_text(json.dumps(manifest,indent=2)+'\n')
old=load(U/'session-owning-source-hashes.json');paths={r['path']for r in old['sources']};paths.update(p for p in load(B/'owned-source-plan.json')['paths']if p.endswith('.rs'));paths.add('docs/specifications/windows/native-token-scalars/cases.json')
rows=[]
for name in sorted(paths):
 raw=(R/name).read_bytes();rows.append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
old.update({'sources':rows,'epoch':'native-token-scalars-final','feature':'Captured scalar token-query contracts','source-parent-rax':manifest['base']})
for p in [B/'scalar-owning-source-hashes.json',S/'scalar-owning-source-hashes.json']:p.write_text(json.dumps(old,indent=2)+'\n')
cpp=load(U/'current-cpp-consumers-session-root.json');cpp['previously-verified-root-head']=cpp['current-root-head'];cpp['current-root-head']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=M,text=True).strip();cpp['current-abi-minor']=int(re.search(r'#define ASSIST_RS_ABI_MINOR (\d+)u',(M/'rust/assist-rs/include/assist_rs.h').read_text())[1])
for row in cpp['files']:
 raw=(M/row['path']).read_bytes()
 if(len(raw),hashlib.sha256(raw).hexdigest())!=(row['bytes'],row['sha256']):raise RuntimeError('selected CPP changed '+row['path'])
for p in [B/'current-cpp-consumers-scalar-root.json',S/'current-cpp-consumers-scalar-root.json']:p.write_text(json.dumps(cpp,indent=2)+'\n')
for oldname,newname in [('run-session-final.py','run-scalar-final.py'),('run-session-owning-gates.py','run-scalar-owning-gates.py'),('collect-session-archives.py','collect-scalar-archives.py')]:
 text=(U/oldname).read_text().replace(str(U),str(B)).replace('session-final','scalar-final').replace('session-owning','scalar-owning').replace('collect-session-archives','collect-scalar-archives').replace('native-token-session-id-final','native-token-scalars-final');(B/newname).write_text(text)
(B/'owning-linux-cpp-build.py').write_text((U/'owning-linux-cpp-build.py').read_text())
for h in ['macos','linux']:
 text=(U/(h+'-session-owning-commands.json')).read_text().replace(str(U),str(B)).replace('session-owning','scalar-owning').replace('session-final','scalar-final').replace('collect-session-archives','collect-scalar-archives');(B/(h+'-scalar-owning-commands.json')).write_text(text)
link=B.parent/'wait-completion-packet-create-20261012/assist-root-current/vendor/rax'
if not link.is_symlink()or link.resolve()!=(U/'rax-source').resolve():raise RuntimeError('unexpected frozen RAX link')
before=os.readlink(link);link.unlink();link.symlink_to(R)
(B/'scalar-owning-unix-link.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'before':before,'after':str(R),'selected-inputs':len(rows)},indent=2)+'\n')
print('Prepared final scalar source',len(manifest['files']),'owning',len(rows),'current Rust ABI',cpp['current-abi-minor'])
