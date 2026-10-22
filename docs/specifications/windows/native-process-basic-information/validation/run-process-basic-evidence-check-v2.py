from pathlib import Path
import hashlib,json,subprocess,tarfile
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
R=Path('C:/Windows/Temp/assist-native-rax-20261010/src-display-candidate1/docs/specifications/windows/native-process-basic-information')
a=S/'process-basic-evidence-check-v2.tar.gz'
assert hashlib.sha256(a.read_bytes()).hexdigest()=='0c16df3d43e79b37808e4873c892c391b1f23baebcf787e1e318e2477bf96d79'
with tarfile.open(a,'r:gz') as t:
 for m in t.getmembers():
  assert m.isfile() and not Path(m.name).is_absolute() and '..' not in Path(m.name).parts
  raw=t.extractfile(m).read();p=R/m.name
  if p.exists():assert p.read_bytes()==raw,m.name
  else:p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(raw)
subprocess.run(['C:/Windows/Temp/assist-native-rax-20261010/python/python.exe','-B',str(R/'check_native_evidence.py'),'--live-native'],check=True)
