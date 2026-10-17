from pathlib import Path
import datetime
import hashlib
import json
import os

BASE = Path('C:/Windows/Temp/assist-native-rax-20261010')
SHARE = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    require(__debug__, 'optimized verification rejected')
    for name in ['access-check-final-launch.json']:
        require(json.loads((SHARE / name).read_text())['exit-code'] == 0, 'completed parent validation')
    config = BASE / 'src/.cargo/config.toml'
    require(sha(config) == '9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c',
            'preserve native Cargo adapter')
    require(sha(Path('C:/Windows/System32/ntdll.dll')) ==
            '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f',
            'installed native NTDLL changed; remeasure before applying build-specific contracts')
    overlay = json.loads((SHARE / 'wnf-query-initial-overlay.json').read_text())
    require(overlay['base'] == '7279261c3ab518e0d0b027817cabf776e9700eb8' and len(overlay['files']) == 11,
            'exact published parent and eleven-input delta')
    for item in overlay['files']:
        name = item['path']
        require(not Path(name).is_absolute() and '..' not in Path(name).parts, 'relative owned path')
        path = BASE / 'src' / name
        previous = sha(path) if path.exists() else None
        require(previous == item['previous-sha256'], 'native baseline input changed: ' + name)
        data = item['text'].encode()
        require((len(data), hashlib.sha256(data).hexdigest()) == (item['bytes'], item['sha256']),
                'frozen overlay bytes: ' + name)
    # Validate all old bytes before writing any task-owned path.
    for item in overlay['files']:
        path = BASE / 'src' / item['path']
        path.parent.mkdir(parents=True, exist_ok=True)
        temporary = path.with_name(path.name + '.wnf-query-owned-temporary')
        require(not temporary.exists(), 'do not replace an earlier partial write')
        temporary.write_bytes(item['text'].encode())
        os.replace(temporary, path)
        os.utime(path, None)
    manifest = json.loads((SHARE / 'wnf-query-initial-owning-source-hashes.json').read_text())
    for item in manifest['sources']:
        path = BASE / 'src' / item['path']
        require((path.stat().st_size, sha(path)) == (item['bytes'], item['sha256']),
                'selected native source: ' + item['path'])
    result = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'base': overlay['base'], 'files': [{k: v for k, v in item.items() if k != 'text'}
                                               for item in overlay['files']],
              'selected-source-count': len(manifest['sources']),
              'native-cargo-adapter-preserved': sha(config),
              'scope': 'ten WNF query Rust inputs and one fixture atop published access-check source; selected325 inputs'}
    (SHARE / 'native-wnf-query-initial-source-verification.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result), flush=True)


if __name__ == '__main__':
    main()
