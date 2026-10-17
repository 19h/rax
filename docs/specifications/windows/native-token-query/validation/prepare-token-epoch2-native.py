from pathlib import Path
import json,hashlib
B=Path(__file__).resolve().parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
old=json.loads((B/'epoch1-code-inputs.json').read_text());new=json.loads((B/'epoch2-code-inputs.json').read_text());pairs=[(a,b) for a,b in zip(old['files'],new['files']) if a['sha256']!=b['sha256']];assert len(pairs)==1
a,b=pairs[0];path=b['path'];before=(B/'rax-source'/path).read_text();after=(B/'rax-final-source'/path).read_text();marker='#[cfg(windows)]\n#[test]\nfn native_token_query_installed_leaves'
assert before.split(marker)[0]==after.split(marker)[0]
(B/'unix-source-equivalence.json').write_text(json.dumps({'path':path,'only-changed-item':'#[cfg(windows)] native_token_query_installed_leaves_replay_every_record_on_available_profiles','non-windows-prefix-sha256':hashlib.sha256(before.split(marker)[0].encode()).hexdigest(),'old-sha256':a['sha256'],'new-sha256':b['sha256'],'other-inputs-identical':len(old['files'])-1},indent=2)+'\n')
(S/'token-query-epoch2-delta.json').write_text(json.dumps({'path':path,'previous-sha256':a['sha256'],'sha256':b['sha256'],'bytes':b['bytes'],'text':after},indent=2)+'\n')
(S/'token-query-epoch2-owning-source-hashes.json').write_bytes((B/'epoch2-owning-source-hashes.json').read_bytes())
for name in ['assist-native-token-query-final-owned.ps1','verify-token-query-final-owning-sources.py','inspect-token-query-final-archives.py']:
 (S/name.replace('token-query-final','token-query-epoch2')).write_bytes((S/name).read_bytes().replace(b'token-query-final',b'token-query-epoch2'))
oldps=(S/'native-token-query-final-gates.ps1').read_text();suffix=oldps[oldps.index('$results=@()'):]
prefix=oldps[:oldps.index("& cmd.exe /d /c ('cargo.exe build")]
newps=(prefix+suffix).replace('token-query-final','token-query-epoch2')
(S/'native-token-query-epoch2-gates.ps1').write_text(newps)
driver=(S/'run-token-query-final-atomic.py').read_text().replace('token-query-final','token-query-epoch2')
(S/'run-token-query-epoch2-atomic.py').write_text(driver)
