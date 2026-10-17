"""Freeze the tested token epoch and prepare the scoped carrier inputs."""
from pathlib import Path
import datetime,hashlib,json,os,re,subprocess
B=Path(__file__).resolve().parent;R=B/'rax-source';D=B.parent/'native-dynamic-function-table-20261017';S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');M=Path('/Users/int/hexrays/kvasir')
manifest=json.loads((B/'token-epoch1-source.json').read_text())
for row in manifest['files']:
 raw=(R/row['path']).read_bytes()
 if len(raw)!=row['bytes'] or hashlib.sha256(raw).hexdigest()!=row['sha256']:raise ValueError('source changed after token focus '+row['path'])
for p in [B/'token-final-source.json',S/'token-final-source.json']:p.write_text(json.dumps(manifest,indent=2)+'\n')
old=json.loads((D/'dynamic-owning-source-hashes.json').read_text());paths={r['path']for r in old['sources']}
paths.update(p for p in json.loads((B/'owned-source-plan.json').read_text())['paths']if p.endswith('.rs'))
paths.update(str(p.relative_to(R)) for p in (R/'docs/specifications/windows/native-process-token-open/native').glob('*.log'))
rows=[]
for path in sorted(paths):
 raw=(R/path).read_bytes();rows.append({'path':path,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
old.update({'sources':rows,'epoch':'process-token-open-final','feature':'Captured primary-token access and guest-owned opening','source-parent-rax':manifest['base'],'scope':'Selected token core, fixture and embedding inputs; frozen carrier, not whole current root'})
for p in [B/'token-owning-source-hashes.json',S/'token-owning-source-hashes.json']:p.write_text(json.dumps(old,indent=2)+'\n')
cpp=json.loads((D/'current-cpp-consumers-dynamic-root.json').read_text());cpp['previously-verified-root-head']=cpp['current-root-head'];cpp['current-root-head']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=M,text=True).strip();cpp['current-abi-minor']=int(re.search(r'#define ASSIST_RS_ABI_MINOR (\d+)u',(M/'rust/assist-rs/include/assist_rs.h').read_text())[1])
for row in cpp['files']:
 raw=(M/row['path']).read_bytes()
 if len(raw)!=row['bytes'] or hashlib.sha256(raw).hexdigest()!=row['sha256']:raise ValueError('current C++ drift '+row['path'])
for p in [B/'current-cpp-consumers-token-root.json',S/'current-cpp-consumers-token-root.json']:p.write_text(json.dumps(cpp,indent=2)+'\n')
text=(S/'verify-current-cpp-consumers.py').read_text().replace('current-cpp-consumers-reverified.json','current-cpp-consumers-token-root.json').replace('native-secure-capture-current-cpp-verification.json','native-token-final-current-cpp-verification.json');(S/'verify-token-cpp-consumers.py').write_text(text)
p=S/'assist-native-token-final-owned.ps1';p.write_text(p.read_text().replace('verify-current-cpp-consumers.py','verify-token-cpp-consumers.py'))
text=(S/'verify-dynamic-owning-sources.py').read_text().replace('src-dynamic-table','src-token-open').replace('dynamic-owning','token-owning').replace('361 selected',str(len(rows))+' selected');(S/'verify-token-owning-sources.py').write_text(text)
text=(S/'prepare-dynamic-owning-junction.py').read_text().replace('src-dynamic-table','src-token-open').replace('src-system-time','src-dynamic-table').replace('dynamic-owning','token-owning');(S/'prepare-token-owning-junction.py').write_text(text)
frozen=B.parent/'wait-completion-packet-create-20261012/assist-root-current/vendor/rax'
if not frozen.is_symlink() or frozen.resolve()!=(D/'rax-source').resolve():raise ValueError('unexpected frozen Unix RAX link')
before=os.readlink(frozen);frozen.unlink();frozen.symlink_to(R)
(B/'token-owning-unix-link.json').write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'before':before,'after':str(R),'selected-inputs':len(rows)},indent=2)+'\n')
print('Prepared final source and',len(rows),'selected owning inputs; current Rust ABI',cpp['current-abi-minor'])
