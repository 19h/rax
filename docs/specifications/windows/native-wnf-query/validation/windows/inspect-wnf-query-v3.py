"""Independently inspect each actual native WNF producer and its output."""
from pathlib import Path
import hashlib, json, re, struct

B = Path('C:/Windows/Temp/assist-native-rax-20261010')
S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
PREFIX = 'native-wnf-query-v3'
SOURCE = '0735731cd8b2bce2f4f2a73847c7f9f316aee1f6db28137c3fbb4b744c770727'

def need(value, message):
    if not value:
        raise ValueError(message)

def identity(path):
    raw = path.read_bytes()
    return {'path': str(path), 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}

def main():
    need(__debug__, 'optimized inspection rejected')
    record = json.loads((S / 'wnf-query-v3-native-launch.json').read_text())
    need(record.get('exit-code') == 0 and 'completed-utc' in record, 'all producers must finish first')
    producers = []
    for profile, machine, width, laa in [('arm64', 0xAA64, 8, True), ('x64', 0x8664, 8, True),
                                         ('x86', 0x14C, 4, False), ('x86-laa', 0x14C, 4, True)]:
        root = B / ('wnf-query-v3-' + profile)
        source = root / (PREFIX + '.cpp')
        binary = root / (PREFIX + '-' + profile + '.exe')
        log = root / (PREFIX + '-' + profile + '.log')
        need(identity(source)['sha256'] == SOURCE, 'actual source identity')
        raw = binary.read_bytes()
        need(raw[:2] == b'MZ' and len(raw) >= 64, 'DOS header')
        pe = struct.unpack_from('<I', raw, 60)[0]
        need(raw[pe:pe+4] == b'PE\0\0', 'PE signature')
        actual_machine, sections = struct.unpack_from('<HH', raw, pe + 4)
        characteristics = struct.unpack_from('<H', raw, pe + 22)[0]
        magic = struct.unpack_from('<H', raw, pe + 24)[0]
        need(actual_machine == machine and sections > 0 and bool(characteristics & 0x20) == laa
             and magic == (0x20B if width == 8 else 0x10B), 'actual producer architecture')
        text = log.read_text(encoding='utf-8-sig')
        need(f'profile={profile} pointer-bytes={width} build=10.0.29683' in text, 'runtime producer identity')
        rows = [dict(pair.split('=', 1) for pair in line.split()[1:])
                for line in text.splitlines() if line.startswith('case ')]
        need(len(rows) == 1920 and [int(r['ordinal']) for r in rows] == list(range(1920)), 'complete call sequence')
        need(all(r['escaped'] == '00000000' and r['last-error'] == '7E11ABCD' and len(r['bytes']) == 128 for r in rows), 'native byte/exception/last-error records')
        deletes = [line for line in text.splitlines() if line.startswith('delete ')]
        need(deletes == [f'delete index={i} status=00000000' for i in range(3)], 'explicit state deletion')
        need('complete cases=1920 deleted-states=3 released-arenas=1 released-descriptors=1 closed-handles=1' in text, 'complete resource cleanup')
        producers.append({'profile': profile, 'machine': machine, 'large-address-aware': laa,
                          'source': identity(source), 'executable': identity(binary), 'stdout': identity(log),
                          'calls': len(rows), 'escaped-exceptions': 0, 'deleted-states': 3,
                          'released-arenas': 1, 'released-descriptors': 1, 'closed-handles': 1})
    result = {'scope': 'four producer ABIs on one ARM64 Windows kernel; private temporary System-scope states and controlled read-only names', 'producers': producers}
    (S / (PREFIX + '-identities.json')).write_text(json.dumps(result, indent=2) + '\n')
    print('Verified four actual producers, 7680 calls and all owned resource cleanup')

if __name__ == '__main__':
    main()
