"""Reject corrupt or overstated validation records in disposable copies."""
from pathlib import Path
import contextlib, hashlib, io, json, shutil, tempfile

B = Path(__file__).resolve().parent
R = B / 'rax-source/docs/specifications/windows/native-access-check'

def main():
    scope = {'__name__': 'access_validation_stress', '__file__': str(R / 'validation-check.py')}
    exec(compile((R / 'validation-check.py').read_text(), str(R / 'validation-check.py'), 'exec'), scope)
    cases = []
    for kind in ['raw-corruption', 'empty-native-gates', 'forged-zero-failures', 'incomplete-owning-driver', 'missing-final-unix-gates']:
        with tempfile.TemporaryDirectory(prefix='access-evidence-') as directory:
            v = Path(directory) / 'validation'
            shutil.copytree(R / 'validation', v)
            manifest = json.loads((v / 'results.json').read_text())
            def update(name):
                p = v / name
                rows = [x for x in manifest['files'] if x['path'] == name]
                if len(rows) != 1:
                    raise ValueError('missing unique manifest entry ' + name)
                rows[0].update(bytes=p.stat().st_size, sha256=hashlib.sha256(p.read_bytes()).hexdigest())
            if kind == 'raw-corruption':
                p = v / 'windows/native-access-check-final-full.log'
                p.write_bytes(p.read_bytes() + b'corruption\n')
            elif kind == 'empty-native-gates':
                n = 'windows/native-access-check-final-verified-gates.json'
                p = v / n
                record = json.loads(p.read_text()); record['gates'] = []
                p.write_text(json.dumps(record)); update(n)
            elif kind == 'forged-zero-failures':
                n = 'windows/native-access-check-final-full.log'; p = v / n
                raw = p.read_bytes()
                if b'7274 passed; 5 failed;' not in raw:
                    raise ValueError('expected native result is absent')
                p.write_bytes(raw.replace(b'7274 passed; 5 failed;', b'7279 passed; 0 failed;'))
                update(n)
                name = 'windows/native-access-check-final-verified-gates.json'; q = v / name
                record = json.loads(q.read_text())
                for gate in record['gates']:
                    if gate['gate'] == 'full':
                        gate['sha256'] = hashlib.sha256(p.read_bytes()).hexdigest()
                q.write_text(json.dumps(record)); update(name)
            elif kind == 'incomplete-owning-driver':
                n = 'windows/access-check-final-launch.json'; p = v / n
                record = json.loads(p.read_text()); record['state'] = 'running-owning'
                p.write_text(json.dumps(record)); update(n)
            else:
                n = 'linux-access-final-gates.json'; p = v / n
                p.write_text('[]\n'); update(n)
            if kind != 'raw-corruption':
                (v / 'results.json').write_text(json.dumps(manifest))
            scope['V'] = v
            try:
                with contextlib.redirect_stdout(io.StringIO()):
                    scope['main']()
            except Exception as error:
                cases.append({'case': kind, 'rejected': True, 'exception': str(error)})
            else:
                raise ValueError('accepted altered evidence ' + kind)
    (B / 'access-validation-stress-results.json').write_text(json.dumps(cases, indent=2) + '\n')
    print(json.dumps(cases))

if __name__ == '__main__':
    main()
