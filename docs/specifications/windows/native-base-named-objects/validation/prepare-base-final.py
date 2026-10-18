from pathlib import Path
import datetime,hashlib,json,subprocess,os,re,shutil
B=Path(__file__).resolve().parent;R=B/'rax-source';W=B.parent/'native-token-scalar-properties-20261018';S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');M=Path('/Users/int/hexrays/kvasir')
def load(p):
 raw=p.read_bytes();return json.loads(raw.decode('utf-16')if raw[:2]in[b'\xff\xfe',b'\xfe\xff']else raw.decode('utf-8-sig'))
for host in ['macos','linux']:
 if load(B/('prototype-'+host+'-focus-result.json'))['exit-code']!=0:raise RuntimeError('prototype focus failed '+host)
focus=load(S/'base-directory-focus-launch.json')
if focus['state']!='finished' or focus['exit_code']!=0:raise RuntimeError('native focus incomplete')
manifest=load(B/'base-directories-epoch1-source.json')
for row in manifest['files']:
 raw=(R/row['path']).read_bytes()
 if len(raw)!=row['bytes']or hashlib.sha256(raw).hexdigest()!=row['sha256']:raise RuntimeError('ported source changed '+row['path'])
manifest.update(base='be05c4b474dbf76be910e0f50dbc4be3262c16ce',epoch='native-base-directories-final',basis='Real worktree port matches all tested prototype/native prepared source bytes.')
for root in [B,S]:(root/'base-final-source.json').write_text(json.dumps(manifest,indent=2)+'\n')
old=load(W/'scalar-owning-source-hashes.json');paths={r['path']for r in old['sources']}
paths.update(path for path in load(B/'owned-source-plan.json')['paths'] if path.endswith('.rs'));paths.update(str(p.relative_to(R))for p in (R/'docs/specifications/windows/native-base-named-objects').glob('*.log'))
rows=[]
for name in sorted(paths):
 raw=(R/name).read_bytes();rows.append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
old.update(sources=rows,epoch='native-base-directories-final',feature='Selected base named-object directory namespace',**{'source-parent-rax':manifest['base']})
for root in [B,S]:(root/'base-owning-source-hashes.json').write_text(json.dumps(old,indent=2)+'\n')
cpp=load(W/'current-cpp-consumers-scalar-root.json');cpp['previously-verified-root-head']=cpp['current-root-head'];cpp['current-root-head']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=M,text=True).strip();cpp['current-abi-minor']=int(re.search(r'#define ASSIST_RS_ABI_MINOR (\d+)u',(M/'rust/assist-rs/include/assist_rs.h').read_text())[1])
for row in cpp['files']:
 raw=(M/row['path']).read_bytes()
 if len(raw)!=row['bytes']or hashlib.sha256(raw).hexdigest()!=row['sha256']:raise RuntimeError('selected C++ changed '+row['path'])
for root in [B,S]:(root/'current-cpp-consumers-base-root.json').write_text(json.dumps(cpp,indent=2)+'\n')
def transform(text):
 return text.replace(str(W),str(B)).replace('src-token-scalars','__NEW_SOURCE__').replace('src-token-session-id','src-token-scalars').replace('__NEW_SOURCE__','src-base-named-objects').replace('scalar-final','base-final').replace('scalar-owning','base-owning').replace('scalar-cpp','base-cpp').replace('scalar-root','base-root').replace('collect-scalar-archives','collect-base-archives').replace('verify-scalar-post-run','verify-base-post-run').replace('native-token-scalars-final','native-base-directories-final')
for name in ['run-scalar-final.py','run-scalar-owning-gates.py','collect-scalar-archives.py']:(B/transform(name)).write_text(transform((W/name).read_text()))
(B/'owning-linux-cpp-build.py').write_bytes((W/'owning-linux-cpp-build.py').read_bytes())
for host in ['macos','linux']:(B/(host+'-base-owning-commands.json')).write_text(transform((W/(host+'-scalar-owning-commands.json')).read_text()))
D=B/'windows';D.mkdir(exist_ok=True)
for name in ['native-scalar-final-core.ps1','run-scalar-final-core.py','run-scalar-final-observations.py','verify-scalar-final.py','assist-native-scalar-final-owned.ps1','run-scalar-final-owned.py','verify-scalar-owning-sources.py','verify-scalar-cpp-consumers.py','inspect-scalar-final-archives.py','verify-scalar-post-run.py','prepare-scalar-owning-junction.py','finish-scalar-evidence.py']:
 new=transform(name).replace('finish-scalar','finish-base');text=transform((W/'windows'/name).read_text()).replace('5718',str(len(manifest['files']))).replace('409 selected',str(len(rows))+' selected').replace('final native token source','final native base-directory source')
 for root in [D,S]:(root/new).write_text(text)
(S/'collect-base-archives.py').write_text((B/'collect-base-archives.py').read_text())
link=B.parent/'wait-completion-packet-create-20261012/assist-root-current/vendor/rax'
if not link.is_symlink()or link.resolve()!=(W/'rax-source').resolve():raise RuntimeError('unexpected frozen Assist RAX link')
before=os.readlink(link);link.unlink();link.symlink_to(R)
(B/'base-owning-unix-link.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'before':before,'after':str(R),'selected-inputs':len(rows)},indent=2)+'\n')
print('Prepared final base namespace:',len(manifest['files']),'core inputs,',len(rows),'owning inputs, root ABI',cpp['current-abi-minor'])
