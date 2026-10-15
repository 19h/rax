from pathlib import Path
import hashlib,json,datetime
root=Path('C:/Windows/Temp/assist-native-rax-20261010/src');share=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
full=json.loads((share/'secure-capture-windows-sources.json').read_text());lookup={r['path']:r for r in full['files']}
overlay=json.loads((share/'secure-capture-overlay.json').read_text())['files'];scoped=json.loads((share/'secure-capture-source-hashes.json').read_text())['sources']
for item in scoped:
 if item['path'] in lookup and lookup[item['path']]!=item:raise ValueError('conflicting scoped identity '+item['path'])
 lookup[item['path']]=item
names={r['path'] for r in overlay+scoped}|{'.cargo/config.toml','Cargo.toml','Cargo.lock'}
rows=[]
for name in sorted(names):
 row=lookup[name];data=(root/name).read_bytes()
 if len(data)!=row['bytes'] or hashlib.sha256(data).hexdigest()!=row['sha256']:raise ValueError('actual input mismatch '+name)
 rows.append(row)
result={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'base':full['base'],'files':rows,'verified-count':len(rows),'verified-bytes':sum(r['bytes'] for r in rows),'scope':'fresh exact overlay and all274 scoped embedding inputs plus Cargo config/manifest/lock; common selection8578 plus4scoped SBPF files have recorded identities; redundant current full-tree native scan interrupted before tests and not claimed complete'}
(share/'native-secure-capture-focused-source-verification.json').write_text(json.dumps(result,indent=2)+'\n')
print('Verified',len(rows),'actual scoped source/input hashes',flush=True)
