from pathlib import Path
import subprocess,sys
b=Path('C:/Windows/Temp/assist-native-rax-20261010');s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
subprocess.run([sys.executable,'-B',str(s/'verify-session-owning-sources.py')],check=True,stdout=sys.stderr)
subprocess.run([sys.executable,'-B',str(s/'collect-session-archives.py'),'--host','windows','--source',str(b/'src-token-session-id'),'--manifest',str(s/'session-owning-source-hashes.json'),'--messages',str(b/'native-session-final-after-artifacts.jsonl'),'--output',str(s/'native-session-final-owning-archive-identities.json')],check=True,stdout=sys.stderr)
print((s/'native-session-final-owning-archive-identities.json').read_text())
