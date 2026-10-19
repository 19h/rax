from pathlib import Path
import subprocess,sys
b=Path('C:/Windows/Temp/assist-native-rax-20261010');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
subprocess.run([sys.executable,'-B',str(s/'verify-wait-owning-sources.py')],check=True,stdout=sys.stderr)
subprocess.run([sys.executable,'-B',str(s/'collect-wait-archives.py'),'--host','windows','--source',str(b/'src-wait-epoch3'),'--manifest',str(s/'wait-owning-source-hashes.json'),'--messages',str(b/'native-wait-final-after-artifacts.jsonl'),'--output',str(s/'native-wait-final-owning-archive-identities.json')],check=True,stdout=sys.stderr)
print((s/'native-wait-final-owning-archive-identities.json').read_text())
