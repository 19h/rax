#!/usr/bin/env python3
"""Check exact historical artifact scope and unchanged feature-owned source."""
from pathlib import Path
import hashlib
import json
import re

ROOT = Path(__file__).resolve().parent
SOURCE = ROOT.parents[3]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def main():
    require(__debug__, 'optimized validation is rejected')
    summary = json.loads((ROOT / 'validation-summary.json').read_text())
    for label, record in summary['results'].items():
        data = (ROOT / record['log']).read_bytes()
        require(hashlib.sha256(data).hexdigest() == record['sha256'], 'recorded gate log ' + label)
        rows = re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out', data.decode(errors='replace'))
        require([list(row) for row in rows] == record['test-results'], 'complete gate selection/count ' + label)
    for host, counts in [('macos', (7797, 0, 2)), ('linux', (7791, 0, 2)), ('windows', (7216, 5, 2))]:
        rows = summary['results'][host + '-lib']['test-results']
        require(len(rows) == 1 and tuple(map(int, rows[0][:3])) == counts, host + ' library outcome')
        rows = summary['results'][host + '-capi']['test-results']
        require(tuple(map(int, rows[0][:2])) == (168, 0), host + ' C API outcome')
        text = (ROOT / summary['results'][host + '-all-targets']['log']).read_text()
        require('Finished `dev` profile' in text and not re.search(r'^error', text, re.M), host + ' all-target compile')
    manifests = []
    for host in ['macos', 'linux', 'windows']:
        recorded = json.loads((ROOT / (host + '-owning-archives.json')).read_text())
        require(recorded['core-features'] == [], host + ' empty core features')
        require(len(recorded['sources']) == 249, host + ' historical scoped source count')
        require(len(recorded['archives']) == 2, host + ' owning archive count')
        for archive in recorded['archives']:
            require(archive['bytes'] > 0 and archive['members'] > 0 and archive['structural-member-walk'] == 'pass', host + ' archive structure')
        manifests.append(recorded['sources'])
    require(manifests[0] == manifests[1] == manifests[2], 'historical source agreement across host embeddings')
    feature = json.loads((ROOT / 'feature-source-hashes.json').read_text())
    require(len(feature['files']) == 24, 'feature-owned Rust source count')
    for row in feature['files']:
        data = (SOURCE / row['path']).read_bytes()
        require((len(data), hashlib.sha256(data).hexdigest()) == (row['bytes'], row['sha256']), 'unchanged current feature source ' + row['path'])
    for host in ['macos', 'linux']:
        text = (ROOT / (host + '-linux-integration-serial.log')).read_text()
        require('test result: ok. 42 passed; 0 failed; 1 ignored;' in text, host + ' complete serial Linux target')
    print('Verified exact3host library/CAPI counts,249historical embedding source hashes,and24current feature-owned Rust sources')


if __name__ == '__main__':
    main()
