from pathlib import Path
import subprocess,sys
s=Path(__file__).parent
for name in ['run-attributes-native-owning-epoch2-d.py','wait-attributes-owner-epoch2.py']:
 subprocess.run([sys.executable,'-B',str(s/name)],check=True)
