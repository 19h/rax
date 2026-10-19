from pathlib import Path
import json
B=Path('C:/Windows/Temp/assist-native-rax-20261010')
print(json.dumps([str(p) for p in B.iterdir() if p.is_dir() and ('target' in p.name or 'build' in p.name)]),flush=True)
