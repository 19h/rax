from pathlib import Path
import hashlib,json
base=Path(r'C:\Windows\Temp\assist-native-rax-20261010')
share=Path(r'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154')
manifest=json.loads((share/'native-timer2-current-overlay-manifest.json').read_text())
roots={'rax':base/'src','root':base/'assist'}
for row in manifest['patches']:
    source=share/row['transfer'];content=source.read_bytes()
    if hashlib.sha256(content).hexdigest()!=row['sha256']:raise SystemExit('transfer mismatch: '+row['path'])
    destination=roots[row['repo']]/row['path'];destination.parent.mkdir(parents=True,exist_ok=True);destination.write_bytes(content)
errors=[]
for row in manifest['source-files'] + manifest['environment-adapters']:
    source=roots[row['repo']]/row['path']
    if not source.is_file():errors.append({'path':row['path'],'reason':'missing'});continue
    content=source.read_bytes()
    if len(content)!=row['bytes'] or hashlib.sha256(content).hexdigest()!=row['sha256']:errors.append({'path':row['path'],'reason':'byte mismatch'})
result={'source-files':len(manifest['source-files']),'patches':len(manifest['patches']),'root-head':manifest['root-head'],'rax-parent':manifest['rax-parent'],'environment-adapters':manifest['environment-adapters'],'errors':errors}
(share/'native-timer2-current-source-check.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result))
if errors:raise SystemExit(1)
