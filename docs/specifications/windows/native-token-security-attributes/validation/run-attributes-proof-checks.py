from pathlib import Path
import subprocess,sys
r=Path(sys.argv[1]).resolve()
subprocess.run([sys.executable,'-B',str(r/'docs/specifications/windows/native-token-security-attributes/check_validation.py'),str(r)],cwd=r,check=True)
