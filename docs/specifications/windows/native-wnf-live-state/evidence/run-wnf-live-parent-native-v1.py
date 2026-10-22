from pathlib import Path
import hashlib,json,subprocess
S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
p=json.loads((S/'process-basic-candidate1-windows-resume1.json').read_bytes())
image=Path(p['test-artifact']['executable'])
raw=image.read_bytes();assert (len(raw),hashlib.sha256(raw).hexdigest())==(p['test-image-before']['bytes'],p['test-image-before']['sha256'])
name='user::windows::wnf::query_tests::native_query_snapshot_captures_typed_state_and_initializes_private_process_state'
result=subprocess.run([str(image),name,'--exact','--nocapture','--test-threads=1'],capture_output=True,text=True)
assert image.read_bytes()==raw
record={'state':'finished','parent':'d5d8ea486c1c4e1b8be8d80d0be7698fa7200b02','image':p['test-image-before'],'exit-code':result.returncode,'stdout':result.stdout,'stderr':result.stderr}
(S/'wnf-live-parent-native-v1.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record))
