"""Reject coherent corruption after rebinding the enclosing byte ledger."""
from pathlib import Path
import copy
import hashlib
import importlib.util
import json
import shutil
import tempfile

CACHE = Path(__file__).parent
ROOT = Path('/Users/int/.assist-namespace-20261020')
DOCS = ROOT / 'docs/specifications/windows/native-namespace-diagnostics'


def load(path):
    return json.loads(path.read_text())


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def envelope(docs):
    rows = []
    for path in sorted(docs.rglob('*')):
        if path.is_file() and path.name != 'evidence-manifest.json':
            raw = path.read_bytes()
            rows.append({'path': path.relative_to(docs).as_posix(), 'bytes': len(raw),
                         'sha256': hashlib.sha256(raw).hexdigest()})
    write(docs / 'evidence-manifest.json', {'files': rows})


def run():
    spec = importlib.util.spec_from_file_location('namespace_proof', DOCS / 'check_validation.py')
    checker = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(checker)
    original_identity = checker.identity
    source_cache = {}

    def identity(path):
        # Only immutable source identities are cached; every mutated proof byte is read anew.
        if path.is_relative_to(ROOT) and not path.is_relative_to(DOCS):
            if path not in source_cache:
                source_cache[path] = original_identity(path)
            return source_cache[path]
        return original_identity(path)

    checker.identity = identity
    checker.check('macos', ROOT)
    results = []
    with tempfile.TemporaryDirectory(prefix='namespace-proof-', dir=ROOT / 'target') as directory:
        trial = Path(directory) / 'proof'
        shutil.copytree(DOCS, trial)
        checker.DOCS = trial
        checker.VALIDATION = trial / 'validation'
        saved = {p.relative_to(trial): p.read_bytes() for p in trial.rglob('*') if p.is_file()}

        def control(name, mutate):
            for relative, raw in saved.items():
                (trial / relative).write_bytes(raw)
            mutate(checker.VALIDATION)
            envelope(trial)
            try:
                checker.check('macos', ROOT)
            except AssertionError:
                results.append({'name': name, 'rejected': True})
            else:
                raise AssertionError('accepted corruption: ' + name)

        def json_change(v, name, mutate):
            path = v / name
            value = load(path)
            mutate(value)
            write(path, value)

        def native_log(v, mutate):
            path = v / 'native-namespace-case-v1-launch.json'
            receipt = load(path)
            row = next(x for x in receipt['steps'] if x['name'] == 'arm64')
            log = v / row['log']
            raw = mutate(log.read_text()).encode()
            log.write_bytes(raw)
            row.update({'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
            write(path, receipt)

        control('canonical-alias-identity', lambda v: native_log(v, lambda s: s.replace(
            'role=private-alias returned=134', 'role=private-alias returned=134').replace(
            s.split('object role=private-alias ')[1].split('name=')[1].splitlines()[0],
            '5C006D006900780065006400')))
        control('temporary-handle-leak', lambda v: native_log(v, lambda s: s.replace(
            'handle-count-after=' + s.split('handle-count-after=')[1].strip(), 'handle-count-after=999')))
        control('fixed-root-type', lambda v: native_log(v, lambda s: s.replace(
            'type=4400690072006500630074006F0072007900', 'type=530079006D0062006F006C00690063004C0069006E006B00', 1)))
        control('kernel-scope', lambda v: json_change(v, 'namespace-kernel-profile.json',
            lambda x: x.update({'native-processor-architecture': 9})))
        control('source-identity', lambda v: json_change(v, 'namespace-source-epoch1.json',
            lambda x: x['files'][0].update({'sha256': '0' * 64})))
        control('new-windows-failure', lambda v: json_change(v, 'gates-windows.json',
            lambda x: x['exact-existing-failures'].append('new::failure')))
        def features(v):
            for name in ['archives-windows-before.json', 'archives-windows-after.json']:
                json_change(v, name, lambda x: x.update({'core-features': ['jit']}))
        control('coherent-owning-feature-drift', features)
        control('missing-falsification-probe', lambda v: json_change(v, 'namespace-ownership.json',
            lambda x: x['assumptions'][0].pop('falsification-probe')))
        control('invented-ci-cause', lambda v: json_change(v, 'summary.json',
            lambda x: x.update({'namespace-cause-on-x64-runner': 'case mismatch'})))
        control('false-full-goal-completion', lambda v: json_change(v, 'summary.json',
            lambda x: x.update({'full-process-goal-complete': True})))
    value = {'controls': results, 'all-rejected': True,
             'scope': 'Each relevant log/receipt and outer envelope is rebound; checker semantics reject the corrupted result.'}
    write(CACHE / 'namespace-proof-negative-controls.json', value)
    write(DOCS / 'validation/negative-controls.json', value)
    shutil.copy2(Path(__file__), DOCS / 'validation' / Path(__file__).name)
    print(json.dumps(value, indent=2), flush=True)


if __name__ == '__main__':
    run()
