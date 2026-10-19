from pathlib import Path
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys
import time


O = Path(__file__).resolve().parent
R = Path('/Users/int/.assist-token-20261019')
HOST = sys.argv[1]
assert HOST in ('macos', 'linux')
RECORD = O / (HOST + '-attributes-epoch2-owning-gates.json')
assert not RECORD.exists()
manifest = json.loads((O / 'attributes-owning-source-hashes-epoch2.json').read_text())
commands = json.loads((O / (HOST + '-attributes-epoch2-owning-commands.json')).read_text())
state = {'state': 'running', 'host': HOST, 'pid': os.getpid(), 'source-inputs': len(manifest['sources']),
         'scope': 'Retained Assist Rust ABI184; selected C++ consumers, not whole current-root products.', 'steps': []}


def verify():
    for row in manifest['sources']:
        raw = (R / row['path']).read_bytes()
        assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], row['path']


for row in commands:
    verify()
    log = O / (HOST + '-attributes-epoch2-owning-' + row['gate'] + '.log')
    start = time.monotonic()
    with log.open('wb') as out:
        environment = dict(os.environ, RUSTUP_TOOLCHAIN='1.95.0')
        if HOST == 'macos':
            environment['TMPDIR'] = str(Path('/Users/int/.assist-token-20261019/t') if row['gate'] in ['ctest', 'protected-assist-cli', 'protected-assist-mcp'] else O.parent / 'wait-completion-packet-create-20261012/assist-root-current/build-native-attributes/tmp')
        child = subprocess.Popen(row['command'], cwd=row.get('cwd'), stdout=out,
                                 stderr=subprocess.STDOUT, env=environment)
        state.update({'current-step': row['gate'], 'child-pid': child.pid})
        RECORD.write_text(json.dumps(state, indent=2) + '\n')
        code = child.wait()
    verify()
    raw = log.read_bytes()
    state['steps'].append({**row, 'exit-code': code, 'seconds': time.monotonic() - start,
                           'log': log.name, 'sha256': hashlib.sha256(raw).hexdigest(), 'bytes': len(raw)})
    state.update({'state': 'failed' if code else 'running', 'exit-code': code})
    RECORD.write_text(json.dumps(state, indent=2) + '\n')
    print(HOST, 'owning', row['gate'], code, flush=True)
    assert code == 0, row['gate']
before = json.loads((O / (HOST + '-attributes-epoch2-pre-cpp-archive-identities.json')).read_text())
after = json.loads((O / (HOST + '-attributes-epoch2-final-owning-archive-identities.json')).read_text())
assert before['archives'] == after['archives'], 'selected archives changed across C++ validation'
if HOST == 'macos':
    build = O.parent / 'wait-completion-packet-create-20261012/assist-root-current/build-native-attributes'
    selected = next(row for row in after['archives'] if row['path'].endswith('.a'))
    cache = (build / 'CMakeCache.txt').read_text()
    configured = re.search(r'^ASSIST_RUST_ARCHIVE_PATH:FILEPATH=(.+)$', cache, re.M).group(1)
    assert Path(configured).resolve() == Path(selected['path']).resolve()
    targets = ['assist_rax_process_tests', 'assist_process_tool_tests', 'assist_rust_archive_link_tests',
               'assist_e14_rax_abi_drift_tests', 'assist_mcp', 'assist_idalib_cli']
    links = []
    for target in targets:
        output = subprocess.check_output(['ninja', '-C', str(build), '-t', 'commands', target], text=True)
        assert selected['path'] in output, target + ' selected carrier absent from actual linker commands'
        links.append({'target': target, 'carrier': selected['path'],
                      'commands-sha256': hashlib.sha256(output.encode()).hexdigest()})
    (O / 'macos-attributes-epoch2-linkage.json').write_text(json.dumps(links, indent=2) + '\n')
state.update({'state': 'finished', 'exit-code': 0, 'completed-utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'archive-identities-stable': True})
RECORD.write_text(json.dumps(state, indent=2) + '\n')
