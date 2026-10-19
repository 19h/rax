"""Independent measured-order hypothesis, checked against native v3/v5 rows."""
from pathlib import Path
import collections
import json

HERE = Path(__file__).resolve().parent
CAPTURES = HERE / "native" if (HERE / "native").is_dir() else HERE
AV, GUARD, ALIGN = 0xC0000005, 0x80000001, 0x80000002
INVALID, INVALID_HANDLE, TYPE, DENIED, NO_TOKEN = (
    0xC000000D, 0xC0000008, 0xC0000024, 0xC0000022, 0xC000007C)


class Fault(Exception):
    def __init__(self, status):
        self.status = status


def predict(row):
    width = int(row['width'])
    nm, hm, om, rm = (int(row[key]) for key in
                       ['names-mode', 'handle-mode', 'output-mode', 'returned-mode'])
    length = int(row['length'], 16)
    source, out, ret = (int(row[key], 16) for key in ['input', 'out-base', 'ret-base'])
    page = 4096
    regions = {source + i * page: 4 for i in range(4)}
    regions.update({out + i * page: 4 for i in range(2)})
    regions.update({ret + i * page: 4 for i in range(2)})
    local = 0x10000
    regions[local] = 4
    if nm in [6, 7]:
        regions[source] = 1 if nm == 6 else 0x104
    if nm in [14, 15]:
        regions[source + 2 * page] = 0x104 if nm == 14 else 1
    if nm == 16:
        regions[source + page] = 1
    if nm == 17:
        regions[source + 3 * page] = 1
    if om in [3, 4, 5, 9, 11]:
        regions[out] = {3: 2, 4: 1, 5: 0x104, 9: 0x102, 11: 0x104}[om]
    if om in [8, 10, 11]:
        regions[out + page] = 0x104 if om == 10 else 1
    if rm in [3, 4, 5]:
        regions[ret] = {3: 2, 4: 1, 5: 0x104}[rm]
    if rm == 9:
        regions[ret + page] = 1
    output = int(row['output'], 16)
    returned = local
    if rm in [1, 2]:
        returned = 0 if rm == 1 else 1
    elif rm in [3, 4, 5, 6]:
        returned = ret + (1 if rm == 6 else 0)
    elif rm in [7, 8]:
        returned = output + (4 if rm == 8 else 0)
    elif rm == 9:
        returned = ret + page - 2
    count = 0 if nm in [1, 19] else 1 if nm == 2 else 2
    names = source
    if nm in [1, 3]:
        names = 0
    elif nm in [4, 19]:
        names = 1
    elif nm == 5:
        names += 1
    elif nm == 16:
        names += page - 4

    def probe(address, size, alignment, write, direct=False):
        if address % alignment:
            raise Fault(ALIGN)
        end = address + size
        while address < end:
            base = address // page * page
            protection = regions.get(base, 1)
            allowed = protection & 0xFF in ([4] if write else [2, 4])
            if direct and not allowed:
                raise Fault(AV)
            if protection & 0x100:
                regions[base] = protection & ~0x100
                raise Fault(GUARD)
            if not allowed:
                raise Fault(AV)
            address = min(end, base + page)

    def descriptors(wow):
        if count == 0:
            if not wow and names != 0:
                raise Fault(INVALID)
            return
        if names == 0:
            if wow:
                return
            raise Fault(INVALID)
        probe(names, count * (8 if wow else 16), 1 if wow else 4, False)

    def strings():
        if count == 0:
            return
        address = source + 2 * page + 64
        size = 32 if nm in [2, 20] else 28
        if nm == 8:
            address = 0
        elif nm == 9:
            address = 1
        elif nm == 10:
            address += 1
        elif nm == 11:
            size -= 1
        elif nm == 13:
            size = 0
        elif nm == 17:
            address = source + 3 * page - 2
        probe(address, size, 2, False)
        if count == 2:
            probe(source + 2 * page + 128, 32 if nm == 20 else 18, 2, False)

    detail = {}
    try:
        if width == 4:
            descriptors(True)
        else:
            if (output == 0) != (length == 0):
                raise Fault(INVALID)
            if length:
                probe(output, length, 4, True)
        probe(returned, 4, 4, True)
        if width == 8:
            descriptors(False)
        elif count and names == 0:
            raise Fault(INVALID)
        strings()
        if hm in [2, 3]:
            raise Fault(INVALID_HANDLE)
        if hm in [4, 6, 7]:
            raise Fault(TYPE)
        if hm == 5:
            raise Fault(DENIED)
        if hm == 8:
            raise Fault(NO_TOKEN)
        detail['returned'] = 0
        if nm == 13:
            status = INVALID
        elif nm not in [1, 2, 19, 20]:
            status = 0xC0000225
        else:
            needed = 192 if nm == 20 else 104
            detail['returned'] = needed
            if length < needed:
                status = 0xC0000023
            elif width == 8 and length < needed + (4 if output % 8 == 4 else 0):
                status = 0x80000005
                detail['packing'] = True
                detail['overflow'] = True
            else:
                status = 0
                detail['packing'] = True
                if width == 4:
                    probe(output, length, 1, True, direct=True)
        raise Fault(status)
    except Fault as result:
        return result.status, regions, detail


def main():
    disagreements = []
    checked = 0
    for version in ['v3', 'v5']:
        for profile in ['arm64', 'x64', 'x86', 'x86-laa']:
            path = CAPTURES / f'native-token-security-attributes-{version}-{profile}.log'
            for line in path.read_text().splitlines():
                if not line.startswith('case='):
                    continue
                row = dict(part.split('=', 1) for part in line.split())
                status, regions, _ = predict(row)
                expected = int(row['status'], 16)
                protection_matches = True
                for key, count in [('input', 4), ('out-base', 2), ('ret-base', 2)]:
                    base = int(row[key], 16)
                    field = {'input': 'input-protect', 'out-base': 'out-protect',
                             'ret-base': 'ret-protect'}[key]
                    actual = [regions[base + i * 4096] for i in range(count)]
                    protection_matches &= actual == [int(v, 16) for v in row[field].split(',')]
                if status != expected or not protection_matches:
                    disagreements.append({'version': version, 'profile': profile,
                                          'case': row['case'], 'predicted': f'{status:08X}',
                                          'expected': row['status'],
                                          'protections-match': protection_matches,
                                          'inputs': {k: row[k] for k in ['label', 'names-mode', 'handle-mode', 'output-mode', 'returned-mode', 'length']}})
                checked += 1
    result = {'checked': checked, 'disagreements': disagreements,
              'hypothesis-confirmed': not disagreements}
    (HERE / 'query-order-hypothesis.json').write_text(json.dumps(result, indent=2) + '\n')
    print('checked', checked, 'disagreements', len(disagreements))
    for row in disagreements[:24]:
        print(json.dumps(row))


if __name__ == '__main__':
    main()
