"""Verify captured native producers and their deterministic replay projection."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re


PROFILES = ('arm64', 'x64', 'x86', 'x86-laa')
PRODUCERS = (
    ('native-v1', 'native-semaphore-v1', 5235,
     '9aed7fd97854147ff95da80f8751d37ac9753004e87270c8818093ff277c8445'),
    ('native-names-v2', 'native-semaphore-names-v2', 224,
     '1c3cbb13d37249f523ff9f00967e85b79b81a319e8005bbb4b92cad1f76d5b47'),
    ('native-query-v3', 'native-semaphore-query-v3', 4608,
     '049fbe4adf09351b0f0cd7f8ce3705eded982b7b52b9aa669566124fe4e70ff3'),
)
NTDLL = {
    '502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f',
    'ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e',
}
AV, GUARD, MISALIGN = 0xC0000005, 0x80000001, 0x80000002


def need(condition, message):
    if not condition:
        raise ValueError(message)


def load(path):
    raw = path.read_bytes()
    encoding = 'utf-16' if raw[:2] in (b'\xff\xfe', b'\xfe\xff') else 'utf-8-sig'
    return json.loads(raw.decode(encoding))


def fields(line):
    return dict(re.findall(r'([\w-]+)=([^\s]*)', line))


def normalized_output(row, width):
    protection, raw = row['output'].split(':')
    data = bytearray.fromhex(raw)
    need(len(data) == 96, 'output capture width')
    if int(row['status'], 16) == 0:
        mode = int(row['om'])
        offset = 0 if mode == 0 else 1 if mode == 1 else 64 if mode < 6 else 60 if mode < 13 else 63
        need(mode not in (6, 7, 8), 'successful invalid output pointer')
        need(int.from_bytes(data[offset:offset + width], 'little') != 0, 'missing successful handle')
        data[offset:offset + width] = bytes.fromhex('0807060504030201')[:width]
    return protection + ':' + data.hex().upper()


def probe(mode, size, nullable=False, aligned=False):
    if mode == 6 and nullable:
        return 0
    if aligned and mode in (1, 7, 8, 13, 14, 15):
        return MISALIGN
    if mode in (2, 3, 6, 7, 8):
        return AV
    if mode in (4, 5):
        return GUARD
    if size > 4 and mode in (10, 11) or mode in (13, 14):
        return AV
    if size > 4 and mode == 12 or mode == 15:
        return GUARD
    return 0


def query_status(row, wow):
    cls, size = int(row['class']), int(row['length'])
    if wow and cls != 0:
        return 0xC0000003
    output = probe(int(row['om']), 8, aligned=True)
    if output:
        return output
    returned = probe(int(row['rm']), 4, nullable=True)
    if returned:
        return returned
    if cls != 0:
        return 0xC0000003
    if size != 8:
        return 0xC0000004
    handle = int(row['hc'])
    if handle in (1, 2):
        return 0xC0000008
    if handle in (3, 4, 5):
        return 0xC0000024
    return 0xC0000022 if handle in (6, 8, 9) else 0


def create_open_status(row, wow, opening):
    attr = int(row['ac'])
    converted = {2: 0xC000000D, 3: 0xC000000D, 5: AV, 7: AV, 8: GUARD, 11: AV}
    if wow and attr in converted:
        return converted[attr]
    mode = int(row['om'])
    output = AV if wow and mode == 5 else probe(mode, 4 if wow else 8)
    if output:
        return output
    if not opening and int(row['pair']) in (2, 3, 4, 5):
        return 0xC000000D
    if not wow and attr in (4, 5):
        return MISALIGN
    if attr in converted:
        return converted[attr]
    if attr == 9:
        return 0xC000000D
    if attr == 10:
        return 0xC0000008 if opening else 0xC0000033
    if opening:
        return 0xC000000D if attr == 0 else 0xC000003B if attr == 1 else 0xC0000034
    return 0


def release_status(row):
    output = probe(int(row['om']), 4, nullable=True)
    if output:
        return output
    if int(row['delta']) <= 0:
        return 0xC000000D
    handle = int(row['hc'])
    if handle in (1, 2):
        return 0xC0000008
    if handle in (3, 4, 5):
        return 0xC0000024
    if handle in (6, 7, 9):
        return 0xC0000022
    return 0 if row['delta'] == '1' else 0xC0000047


def check(root, write=False):
    projections = {profile: {'create': [], 'open': [], 'release': [], 'query': []}
                   for profile in PROFILES}
    report = []
    for directory, stem, cases, source_hash in PRODUCERS:
        base = root / directory
        need(hashlib.sha256((base / (stem + '.cpp')).read_bytes()).hexdigest() == source_hash,
             stem + ' source identity')
        launch = load(base / (stem + '-launch.json'))
        need(launch['state'] == 'finished' and launch['exit-code'] == 0, stem + ' incomplete')
        outcomes = load(base / (stem + '-outcomes.json'))
        need([(r['profile'], r['exit-code']) for r in outcomes] == [(p, 0) for p in PROFILES],
             stem + ' profile outcomes')
        modules = load(base / (stem + '-modules-before.json'))
        need(modules == load(base / (stem + '-modules-after.json')), stem + ' provider drift')
        need(len(modules) == 6 and NTDLL <= {m['Hash'].lower() for m in modules},
             stem + ' provider set')
        for profile in PROFILES:
            label = stem + '-' + profile
            before = load(base / (label + '-before.json'))
            need(before == load(base / (label + '-after.json')), label + ' executable drift')
            need(before['profile'] == profile and before['machine'] ==
                 {'arm64': 43620, 'x64': 34404, 'x86': 332, 'x86-laa': 332}[profile],
                 label + ' executable ABI')
            need(before['large-address-aware'] == (profile != 'x86'), label + ' LAA')
            need(load(base / (label + '-source-after.json'))['Hash'].lower() == source_hash,
                 label + ' post-source identity')
            script = (base / (stem + '.ps1')).read_text()
            need(source_hash in script and '/we4473 /we4474 /we4477' in script,
                 label + ' checked format arguments')
            lines = (base / (label + '.log')).read_text().splitlines()
            header = fields(lines[0])
            width = 4 if profile.startswith('x86') else 8
            need(header['profile'] == profile and int(header['pointer-bytes']) == width,
                 label + ' runtime ABI')
            done = fields(lines[-1])
            need(int(done['cases']) == cases, label + ' case count')
            if directory == 'native-names-v2':
                need(done['acquired'] == done['closed'] and done['before'] == done['after'],
                     label + ' leaked handles')
                rows = [fields(line) for line in lines if line.startswith('named ')]
                need([int(r['ordinal']) for r in rows] == list(range(cases)), label + ' names ordering')
                need(all(fields(line)['escaped'] == '00000000' for line in lines
                         if line.startswith('attribute-pointer ')), label + ' escaped exception')
                bridge = fields(next(line for line in lines if line.startswith('bridge-open ')))
                need(bridge == {'status': '00000000', 'same': '1'}, label + ' Win32/native identity')
                states = [fields(line) for line in lines if line.startswith('bridge-state ')]
                need([int(r['count']) for r in states] == [0, 2, 3, 2] and
                     all(r['max'] == '3' and r['status'] == '00000000' for r in states),
                     label + ' shared count/wait behavior')
                need([r['action'] for r in states] == ['00000000', '00000000', '00000001', '00000000'],
                     label + ' native/Win32 outcomes')
                counts = [r for r in rows if r['label'] == 'existing-count-validation']
                need(len(counts) == 36, label + ' existing count matrix')
                for row in counts:
                    initial, maximum = int(row['initial']), int(row['max'])
                    expected = 0xC000000D if maximum <= 0 or initial < 0 or initial > maximum else (
                        0x40000000 if int(row['flags'], 16) & 0x80 else 0xC0000035)
                    need(int(row['status'], 16) == expected, label + ' existing count validation')
            else:
                need(done['created'] == done['closed'] and done['handles-before'] == done['handles-after'],
                     label + ' leaked handles')
                rows = [(line.split()[0], fields(line)) for line in lines[1:-1]]
                need(len(rows) == cases and [int(r['ordinal']) for _, r in rows] == list(range(cases)),
                     label + ' ordering/count')
                need(all(r['escaped'] == '00000000' for _, r in rows), label + ' escaped exception')
                groups = Counter(kind for kind, _ in rows)
                expected = {'create': 1536, 'access': 16, 'open': 192, 'release': 1120,
                            'query': 1600, 'query-pair': 768, 'query-alias': 3}
                need(dict(groups) == (expected if directory == 'native-v1' else
                                     {'query-precedence': 4608}), label + ' matrix cardinality')
                for kind, row in rows:
                    target = projections[profile]
                    if kind in ('create', 'open'):
                        need(int(row['status'], 16) == create_open_status(row, width == 4, kind == 'open'),
                             label + ' independent create/open count/conversion/probe contract')
                        columns = ([row['ac'], row['pair'], row['om']] if kind == 'create'
                                   else [row['ac'], row['om'], header['pid']])
                        columns += [row['status'], normalized_output(row, width), row['attrs'].split(':')[0],
                                    row.get('created', '-')]
                        target[kind].append(columns)
                    elif kind == 'release':
                        need(int(row['status'], 16) == release_status(row),
                             label + ' independent release fault/count/access contract')
                        need(int(row['count']) == (2 if row['status'] == '00000000' else 1),
                             label + ' release mutated on failure or lost successful update')
                        target['release'].append([row[k] for k in ('hc', 'delta', 'om', 'status', 'count', 'output')])
                    elif kind.startswith('query'):
                        row = dict(row)
                        if kind in ('query-pair', 'query-alias'):
                            row.update({'class': '0', 'length': '8'})
                        if kind == 'query-alias':
                            row.update({'hc': '0', 'om': '0', 'rm': 'a' + row['offset'], 'returned': '-'})
                        else:
                            need(int(row['status'], 16) == query_status(row, width == 4),
                                 label + ' independent query fault/class/length/access contract')
                        target['query'].append([row[k] for k in
                                                ('hc', 'class', 'length', 'om', 'rm', 'status', 'output', 'returned')])
            report.append({'producer': stem, 'profile': profile, 'cases': cases})
    for profile, groups in projections.items():
        for kind, rows in groups.items():
            projected = ''.join('\t'.join(row) + '\n' for row in rows)
            path = root / 'replay' / (kind + '-' + profile + '.tsv')
            if write:
                path.write_text(projected)
            need(path.read_text() == projected, str(path) + ' projection drift')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write-replays', action='store_true')
    parser.add_argument('root', nargs='?', type=Path, default=Path(__file__).resolve().parent)
    args = parser.parse_args()
    print(json.dumps({'native': check(args.root, args.write_replays)}, indent=2))


if __name__ == '__main__':
    main()
