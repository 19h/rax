from pathlib import Path
import json
import runpy
import shutil
import sys

if sys.flags.optimize:
    raise ValueError('optimized final evidence inspection rejected')
share = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
runpy.run_path(str(share / 'inspect-section-final-observer.py'), run_name='__main__')
runpy.run_path(str(share / 'inspect-section-producers.py'), run_name='__main__')
shutil.copyfile(share / 'native-section-producer-identities.json', share / 'native-section-final-producer-identities.json')
identities = json.loads((share / 'native-section-final-producer-identities.json').read_text())
print('after-owning-native-producers', len(identities['producers']))
