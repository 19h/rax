from pathlib import Path
import json,hashlib,subprocess,shutil
B=Path(__file__).resolve().parent;R=B/'rax-source';S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');C=B.parent/'native-cookie-owner-20261016'
parent='aa9bcc5651164ade3265e124de9a6d4f778f561f'
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()==parent
scope=json.loads((B/'token-scope.json').read_text());paths=scope['owned']+['docs/specifications/windows/native-token-query/cases.json'];rows=[]
for name in paths:
 d=(R/name).read_bytes();old=subprocess.run(['git','show','HEAD:'+name],cwd=R,capture_output=True)
 rows.append({'path':name,'bytes':len(d),'sha256':hashlib.sha256(d).hexdigest(),'previous-sha256':hashlib.sha256(old.stdout).hexdigest() if old.returncode==0 else None,'text':d.decode()})
(B/'final-code-inputs.json').write_text(json.dumps({'base':parent,'files':[{k:v for k,v in r.items() if k!='text'} for r in rows]},indent=2)+'\n')
(S/'token-query-final-overlay.json').write_text(json.dumps({'base':parent,'files':rows},indent=2)+'\n')
m=json.loads((C/'final-owning-source-hashes.json').read_text());by={r['path']:r for r in m['sources']}
for r in rows:by[r['path']]={k:r[k] for k in ['path','bytes','sha256']}
m['sources']=[by[p] for p in sorted(by)];assert len(m['sources'])==313
for p in [B/'final-owning-source-hashes.json',S/'token-query-final-owning-source-hashes.json']:p.write_text(json.dumps(m,indent=2)+'\n')
# Reuse the already validated owning commands with exact new source identities.
for name in ['assist-native-cookie-owner-final-restored-owned.ps1','verify-cookie-owner-final-restored-owning-sources.py','inspect-cookie-owner-final-restored-archives.py']:
 text=(S/name).read_text().replace('cookie-owner-final-restored','token-query-final')
 (S/name.replace('cookie-owner-final-restored','token-query-final')).write_text(text)
# Preserve the original observer unchanged.
text=(S/'run-cookie-owner-final-observations.py').read_text().replace('cookie-owner','token-query')
(S/'run-token-query-final-observations.py').write_text(text)
print('Frozen',len(rows),'changed inputs and',len(m['sources']),'selected owning sources')
