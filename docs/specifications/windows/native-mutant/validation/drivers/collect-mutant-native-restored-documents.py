from pathlib import Path
import hashlib,json,shutil
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=Path('C:/Windows/Temp/assist-native-rax-20261010/src-mutant-epoch4')
review=json.loads((S/'mutant-native-restored-inputs-review.json').read_text());out=S/'mutant-native-restored-documents';out.mkdir(exist_ok=True)
for row in review['representation-or-document-differences']:
 if not row['classification'].startswith('Retained'):continue
 p=R/row['path'];raw=p.read_bytes();assert hashlib.sha256(raw).hexdigest()==row['expected']['sha256'];target=out/row['path'];target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(raw)
raw=(R/'.cargo/config.toml').read_bytes();assert hashlib.sha256(raw).hexdigest()=='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c';(S/'mutant-native-cargo-config.toml').write_bytes(raw)
print('Retained nine exact historical document inputs and unchanged Cargo source configuration')
