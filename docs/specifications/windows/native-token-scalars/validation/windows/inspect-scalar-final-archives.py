from pathlib import Path
import subprocess,sys
b=Path('C:/Windows/Temp/assist-native-rax-20261010');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
subprocess.run([sys.executable,'-B',str(s/'verify-scalar-owning-sources.py')],check=True,stdout=sys.stderr)
subprocess.run([sys.executable,'-B',str(s/'collect-scalar-archives.py'),'--host','windows','--source',str(b/'src-token-scalars'),'--manifest',str(s/'scalar-owning-source-hashes.json'),'--messages',str(b/'native-scalar-final-after-artifacts.jsonl'),'--output',str(s/'native-scalar-final-owning-archive-identities.json')],check=True,stdout=sys.stderr)
print((s/'native-scalar-final-owning-archive-identities.json').read_text())
