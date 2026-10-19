from pathlib import Path
import json,time,subprocess,sys
s=Path(__file__).parent
for _ in range(1800):
 d=json.loads((s/'attributes-native-owning-epoch2-b.json').read_text())
 if d['state']=='failed':break
 assert d['state']!='finished','unexpected successful b harness'
 time.sleep(2)
else:raise RuntimeError('prior owning controller did not settle')
subprocess.run([sys.executable,'-B',str(s/'run-attributes-native-owning-epoch2-c.py')],check=True)
subprocess.run([sys.executable,'-B',str(s/'wait-attributes-owner-epoch2.py')],check=True)
