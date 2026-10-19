from pathlib import Path
import json,subprocess,time,sys
s=Path(__file__).resolve().parent
for _ in range(1800):
 p=s/'loader-native-owning-epoch3.json'
 if p.exists():
  j=json.loads(p.read_text())
  assert j['state']!='failed',j.get('error')
  if j['state']=='finished':break
 time.sleep(2)
else:raise RuntimeError('retained owning validation did not complete within one hour')
subprocess.run([sys.executable,'-B',str(s/'run-loader-owner-epoch3.py')],check=True)
