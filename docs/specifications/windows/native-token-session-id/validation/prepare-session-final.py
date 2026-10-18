from pathlib import Path
import datetime,hashlib,json,os,re,subprocess
B=Path(__file__).resolve().parent;R=B/'rax-source';T=B.parent/'native-process-token-open-20261017';S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');M=Path('/Users/int/hexrays/kvasir')
manifest=json.loads((B/'session-id-epoch2-complete-source.json').read_text())
for row in manifest['files']:
 raw=(R/row['path']).read_bytes()
 if (len(raw),hashlib.sha256(raw).hexdigest())!=(row['bytes'],row['sha256']):raise RuntimeError('source drift '+row['path'])
for p in [B/'session-final-source.json',S/'session-final-source.json']:p.write_text(json.dumps(manifest,indent=2)+'\n')
old=json.loads((T/'token-owning-source-hashes.json').read_text());paths={x['path']for x in old['sources']};paths.update(p for p in json.loads((B/'owned-source-plan.json').read_text())['paths']if p.endswith('.rs'));paths.add('docs/specifications/windows/native-token-session-id/cases.json')
rows=[]
for p in sorted(paths):
 raw=(R/p).read_bytes();rows.append({'path':p,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
old.update({'sources':rows,'epoch':'token-session-id-final','feature':'Captured token SessionId and independent process-session PEB identity','source-parent-rax':manifest['base']})
for p in [B/'session-owning-source-hashes.json',S/'session-owning-source-hashes.json']:p.write_text(json.dumps(old,indent=2)+'\n')
cpp=json.loads((T/'current-cpp-consumers-token-root.json').read_text());cpp['previously-verified-root-head']=cpp['current-root-head'];cpp['current-root-head']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=M,text=True).strip();cpp['current-abi-minor']=int(re.search(r'#define ASSIST_RS_ABI_MINOR (\d+)u',(M/'rust/assist-rs/include/assist_rs.h').read_text())[1])
for row in cpp['files']:
 raw=(M/row['path']).read_bytes()
 if (len(raw),hashlib.sha256(raw).hexdigest())!=(row['bytes'],row['sha256']):raise RuntimeError('current CPP changed '+row['path'])
for p in [B/'current-cpp-consumers-session-root.json',S/'current-cpp-consumers-session-root.json']:p.write_text(json.dumps(cpp,indent=2)+'\n')
for name,newname in [('run-token-final.py','run-session-final.py'),('run-token-owning-gates.py','run-session-owning-gates.py'),('collect-token-archives.py','collect-session-archives.py')]:
 text=(T/name).read_text().replace('-token-final-','-session-final-').replace('-token-owning-','-session-owning-').replace('native-process-token-open-final','native-token-session-id-final');(B/newname).write_text(text)
(B/'owning-linux-cpp-build.py').write_text((T/'owning-linux-cpp-build.py').read_text())
for host in ['macos','linux']:
 text=(T/(host+'-token-owning-commands.json')).read_text().replace(str(T),str(B)).replace('token-owning','session-owning').replace('token-final','session-final').replace('collect-token-archives','collect-session-archives');(B/(host+'-session-owning-commands.json')).write_text(text)
link=B.parent/'wait-completion-packet-create-20261012/assist-root-current/vendor/rax'
if not link.is_symlink() or link.resolve()!=(T/'rax-source').resolve():raise RuntimeError('unexpected frozen RAX link')
before=os.readlink(link);link.unlink();link.symlink_to(R)
(B/'session-owning-unix-link.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'before':before,'after':str(R),'selected-inputs':len(rows)},indent=2)+'\n')
print('Prepared final source',len(manifest['files']),'owning',len(rows),'current Rust ABI',cpp['current-abi-minor'])
