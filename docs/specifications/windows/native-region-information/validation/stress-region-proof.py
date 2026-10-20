"""Reject semantic corruption after all relevant evidence hashes are rebound."""
from pathlib import Path
import hashlib
import importlib.util
import json
import shutil
import tempfile

CACHE = Path(__file__).parent
ROOT = Path('/Users/int/.assist-region-20261020')
DOCS = ROOT / 'docs/specifications/windows/native-region-information'

def load(path):
    return json.loads(path.read_text())

def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')

def seal(docs):
    rows = []
    for path in sorted(docs.rglob('*')):
        if path.is_file() and path.name != 'evidence-manifest.json':
            raw = path.read_bytes()
            rows.append({'path': path.relative_to(docs).as_posix(), 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
    write(docs / 'evidence-manifest.json', {'files': rows})

def run():
    spec = importlib.util.spec_from_file_location('region_proof', DOCS / 'check_validation.py')
    checker = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(checker)
    original_identity = checker.identity
    immutable = {}
    def identity(path):
        if path.is_relative_to(ROOT) and not path.is_relative_to(DOCS):
            if path not in immutable:
                immutable[path] = original_identity(path)
            return immutable[path]
        return original_identity(path)
    checker.identity = identity
    checker.check('macos', ROOT)
    results = []
    with tempfile.TemporaryDirectory(prefix='region-proof-', dir=ROOT / 'target') as directory:
        trial = Path(directory) / 'proof'
        shutil.copytree(DOCS, trial)
        checker.DOCS = trial
        checker.VALIDATION = trial / 'validation'
        checker.NATIVE = trial / 'native'
        saved = {p.relative_to(trial): p.read_bytes() for p in trial.rglob('*') if p.is_file()}
        def control(name, mutate):
            for relative, raw in saved.items():
                (trial / relative).write_bytes(raw)
            mutate(trial)
            seal(trial)
            try:
                checker.check('macos', ROOT)
            except AssertionError:
                results.append({'name': name, 'rejected': True})
            else:
                raise AssertionError('accepted corruption: ' + name)
        def change(path, mutate):
            value = load(path)
            mutate(value)
            write(path, value)
        def record_log(d, receipt, name, mutate, native=False):
            base = d / ('native' if native else 'validation')
            path = base / receipt
            value = load(path)
            row = next(row for row in value['steps'] if row.get('name', row.get('gate')) == name)
            target = base / row['log']
            raw = mutate(target.read_text()).encode()
            target.write_bytes(raw)
            row.update({'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
            write(path, value)
        control('native-short-buffer-must-fail', lambda d: record_log(d, 'native-region-query-v16-launch.json', 'arm64',
            lambda s: s.replace('class=3 length=0 status=C0000004', 'class=3 length=0 status=00000000', 1), True))
        control('native-handle-leak', lambda d: record_log(d, 'native-region-query-v10-launch.json', 'arm64',
            lambda s: s.replace('handle-count-after=43', 'handle-count-after=999'), True))
        control('replay-overbroad-mask', lambda d: change(d / 'modelled-query-cases.json',
            lambda value: value['rows'][0]['masked-byte-indices'].append(63)))
        control('replay-missing-observation', lambda d: change(d / 'modelled-query-cases.json',
            lambda value: value['rows'].pop()))
        control('kernel-scope-drift', lambda d: change(d / 'validation/kernel-profile.json',
            lambda value: value.update({'native-processor-architecture': 9})))
        control('source-identity', lambda d: change(d / 'validation/region-source-epoch1.json',
            lambda value: value['files'][0].update({'sha256': '0' * 64})))
        control('new-windows-failure', lambda d: change(d / 'validation/gates-windows.json',
            lambda value: value['exact-existing-failures'].append('new::failure')))
        def features(d):
            for name in ['archives-windows-before.json', 'archives-windows-after.json']:
                change(d / 'validation' / name, lambda value: value.update({'core-features': ['jit']}))
        control('coherent-owning-feature-drift', features)
        control('ordinary-query-failure', lambda d: record_log(d, 'ordinary.json', 'whoami',
            lambda s: s.replace('class=3 status=0x0', 'class=3 status=0xc000000d', 1)))
        control('capi-frontier-as-exit', lambda d: change(d / 'validation/capi.json',
            lambda value: value['outcomes'][1].update({'reason': 4})))
        control('missing-falsification-probe', lambda d: change(d / 'validation/assumptions-final.json',
            lambda value: value['assumptions'][0].pop('falsification-probe')))
        control('false-full-goal-completion', lambda d: change(d / 'validation/summary.json',
            lambda value: value.update({'full-process-goal-complete': True})))
    value = {'controls': results, 'all-rejected': True, 'scope': 'Relevant log/receipt and enclosing hashes rebound; semantic predicates reject all12 coherent corruptions.'}
    write(CACHE / 'region-proof-negative-controls.json', value)
    write(DOCS / 'validation/negative-controls.json', value)
    shutil.copy2(Path(__file__), DOCS / 'validation' / Path(__file__).name)
    print(json.dumps(value, indent=2), flush=True)

if __name__ == '__main__':
    run()
