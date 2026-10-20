"""Verify declared public projections of historical evidence files.

The recorded identity belongs to the original checkpoint. It is returned only
when the current bytes match an explicitly declared projection. This adapter
does not provide the withheld bytes or change any runtime/test result.
"""
from pathlib import Path
import argparse
from functools import lru_cache
import hashlib
import json

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / 'docs/development/evidence-projection/manifest.json'


def actual_identity(path):
    digest = hashlib.sha256()
    size = 0
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b''):
            digest.update(chunk)
            size += len(chunk)
    return size, digest.hexdigest()


@lru_cache(maxsize=1)
def entries():
    value = json.loads(MANIFEST.read_text())
    assert value['base'] == '6f2148297acc1e8106600306cc092b08207b80d2'
    assert value['identity-contract'] == 'original-checkpoint-with-explicit-public-projections'
    paths = set()
    by_content = {}
    for row in value['files']:
        path = Path(row['path'])
        assert row['path'].startswith('docs/specifications/windows/')
        assert not path.is_absolute() and '..' not in path.parts
        assert path.suffix in ['.log', '.jsonl', '.py']
        assert row['kind'] in ['compiler-diagnostic-projection', 'compiler-diagnostic-text-projection',
                               'projection-aware-checker']
        assert row['path'] not in paths
        paths.add(row['path'])
        projected = row['published']['bytes'], row['published']['sha256']
        original = row['original']['bytes'], row['original']['sha256']
        assert projected != original
        assert by_content.setdefault(projected, original) == original
    return value, by_content


def recorded_identity(path):
    """Return checkpoint identity for exact declared projection bytes only."""
    actual = actual_identity(path)
    _, mapping = entries()
    return mapping.get(actual, actual)


def check(root=ROOT):
    value, _ = entries()
    count = 0
    for row in value['files']:
        path = root / row['path']
        assert actual_identity(path) == (row['published']['bytes'], row['published']['sha256']), row['path']
        assert recorded_identity(path) == (row['original']['bytes'], row['original']['sha256'])
        if row['kind'].startswith('compiler-diagnostic-'):
            artifacts = 0
            for line in path.read_text(errors='replace').splitlines():
                try:
                    record = json.loads(line)
                except json.JSONDecodeError:
                    continue
                assert not (record.get('reason') == 'compiler-message'
                            and 'assist-rs' in record.get('package_id', '')), row['path']
                artifacts += record.get('reason') == 'compiler-artifact'
            if row['kind'] == 'compiler-diagnostic-projection':
                assert artifacts > 0
            assert row['withheld-records']
            output = path.read_text(errors='replace')
            assert not ('claude_process' in output and 'warning: unused import:' in output)
            count += 1
    assert count == value['projected-streams'] and count >= 147
    return {'state': 'verified', 'projected-streams': count, 'projection-bindings': len(value['files']),
            'identity-contract': value['identity-contract'], 'runtime-source-changes': False,
            'history-rewritten': False}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', type=Path, default=ROOT)
    arguments = parser.parse_args()
    print(json.dumps(check(arguments.root), indent=2))
