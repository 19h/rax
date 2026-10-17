from pathlib import Path
import json
base=Path('C:/Windows/Temp/assist-native-rax-20261010')
result={'config': {str(p):p.read_text() for p in [base/'assist/.cargo/config.toml',base/'src/.cargo/config.toml'] if p.exists()},'directories':[]}
for p in sorted((base/'assist-dependencies').iterdir()):
 if not p.is_dir(): continue
 check=p/'.cargo-checksum.json'
 r={'name':p.name,'manifest':(p/'Cargo.toml').is_file()}
 if check.exists():
  expected=json.loads(check.read_text()).get('files',{})
  missing=[name for name in expected if not (p/name).is_file()]
  r.update(expected=len(expected),missing=len(missing),examples=missing[:3])
 result['directories'].append(r)
print(json.dumps(result,indent=2))
