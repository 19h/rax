from pathlib import Path
import hashlib,json
b=Path('C:/Windows/Temp/assist-native-rax-20261010');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
paths=[b/'native-token-final-trace.exe',b/'native-shared-section-trace.rs',b/'src-token-open/.cargo/config.toml',b/'assist/src/emulation/rax/rax_process.cpp',Path('C:/Windows/System32/ntdll.dll'),Path('C:/Windows/System32/whoami.exe'),b/'src-token-open/tests/fixtures/user/windows/bin/arm64/smoke.exe']
rows=[]
for p in paths:
 raw=p.read_bytes();rows.append({'path':str(p),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
(s/'native-token-final-post-run-identities.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows))
