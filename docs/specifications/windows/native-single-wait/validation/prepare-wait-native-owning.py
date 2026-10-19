from pathlib import Path
import json

N = Path(__file__).resolve().parent
M = N.parent / 'native-mutant-create-20261018'
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
D = N / 'owning-windows-wait'
D.mkdir(exist_ok=True)
for path in (M / 'owning-windows-mutant').iterdir():
    if not path.is_file():
        continue
    source = path.read_text().replace('mutant', 'wait').replace('src-object-reparse-epoch1', 'src-mutant-epoch4').replace('src-wait-epoch4', 'src-wait-epoch3')
    source = source.replace('target-io-completion-owned', 'target-wait-owned').replace('wait-native-epoch3-focus.log', 'wait-native-epoch3b-focus.log')
    name = path.name.replace('mutant', 'wait')
    if name == 'prepare-wait-owning-junction.py':
        source = source.replace("m = json.loads", "focus = (s / 'wait-native-epoch3b-focus.log').read_text()\nif '16 passed; 0 failed;' not in focus:\n    raise ValueError('native focused validation incomplete')\nm = json.loads")
    if name == 'verify-wait-owning-sources.py':
        source = source.replace('456 selected source/fixture inputs', '473 selected source/fixture inputs')
    if name == 'verify-wait-cpp-consumers.py':
        source = source.replace("'scope':'exact9C++inputs;headers/generated resources/dependencies and whole-root parity are not established by this comparison'", "'scope':'Exact selected C++ inputs; headers/generated resources/dependencies and whole-root parity are not established by this comparison'")
        source = source.replace("print('Verified9 actual frozen Windows C++ sources against compared source snapshot',flush=True)", "print('Verified',len(rows),'actual frozen Windows C++ sources against compared source snapshot',flush=True)")
    for root in [D, S]:
        (root / name).write_text(source)
# The compared C++ files are unchanged by the independent current root port.
meta = json.loads((M / 'current-cpp-consumers-mutant-root.json').read_text())
meta['current-root-head'] = '8f49854be133693b69f546748fe207b23c6a23cd'
meta['current-root-rust-abi-minor'] = 264
import subprocess,hashlib
meta['excluded-independent-file'] = 'src/crypto/static_text.cpp; current Rust static-text bridge differs from frozen carrier'
meta['files'] = [row for row in meta['files'] if row['path'] != 'src/crypto/static_text.cpp']
for row in meta['files']:
    raw = subprocess.check_output(['git','show',meta['current-root-head']+':'+row['path']],cwd='/Users/int/hexrays/kvasir')
    assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'],row['path']
for root in [N,S]:
    (root / 'current-cpp-consumers-wait-root.json').write_text(json.dumps(meta,indent=2)+'\n')
print('Prepared guarded native owning continuation and compared',len(meta['files']),'C++ source inputs')
