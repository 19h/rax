from pathlib import Path
import datetime
import hashlib
import json
import re

BASE = Path(__file__).resolve().parent


def main():
    if not __debug__:
        raise ValueError('optimized verification rejected')
    summary = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
               'source-epoch': 2, 'hosts': {}}
    for host, expected in [('macos', 7836), ('linux', 7830)]:
        folder = BASE / (host + '-epoch2')
        gates = json.loads((folder / 'gates.json').read_text())
        if len(gates) != 7 or any(row['exit-code'] for row in gates):
            raise ValueError('incomplete or failed final Unix gates: ' + host)
        measured = {}
        for row in gates:
            data = (folder / row['log']).read_bytes()
            if hashlib.sha256(data).hexdigest() != row['sha256']:
                raise ValueError('changed raw log: ' + row['log'])
            measured[row['gate']] = [tuple(map(int, values)) for values in re.findall(
                r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;',
                data.decode('utf-8'))]
        if measured['lib'] != [(expected, 0, 2, 0)]:
            raise ValueError('unfiltered final full library: ' + host)
        if (168, 0, 0, 0) not in measured['capi'] or (544, 0, 0, 0) not in measured['windows']:
            raise ValueError('actual C API or integration selection: ' + host)
        if not any(row[:3] == (6, 0, 0) for row in measured['focused']):
            raise ValueError('six actual shared focused tests: ' + host)
        summary['hosts'][host] = {'completed-gates': 7, 'all-exit-codes': 0,
                                  'summaries-passed-failed-ignored-filtered': measured,
                                  'native-windows-memory-execution': False,
                                  'scope': 'shared portable replay on this host; installed Windows leaves remain a separate native gate'}
    (BASE / 'final-unix-core-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
