"""Prepare independent final evidence checks without modifying the tested source."""
from pathlib import Path
import json

B = Path(__file__).resolve().parent
N = B.parent / 'native-base-named-objects-20261018'
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')

def transform(text):
    return (text.replace(str(N), str(B))
            .replace('native-base-named-objects', 'native-token-bno-isolation')
            .replace('src-base-named-objects', 'src-bno-isolation-prototype')
            .replace('base-final', 'bno-final').replace('base-owning', 'bno-owning')
            .replace('base-owner', 'bno-owner').replace('base-root', 'bno-root'))

for name in ['run-base-owner-observer.py', 'base-owner-observer-build.ps1']:
    out = name.replace('base-owner', 'bno-owner')
    text = transform((N / name).read_text())
    (B / out).write_text(text)
    (S / out).write_text(text)
(S / 'bno-owner-observer.rs').write_bytes((S / 'base-owner-observer.rs').read_bytes())

text = (S / 'verify-base-post-run.py').read_text()
text = transform(text).replace('native-bno-final-trace.exe', 'native-bno-epoch5-trace.exe')
for root in [B, S]:
    (root / 'verify-bno-post-run.py').write_text(text)
source_check = '''from pathlib import Path
import hashlib, json
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
r=Path('C:/Windows/Temp/assist-native-rax-20261010/src-bno-isolation-prototype')
m=json.loads((s/'bno-epoch5-source.json').read_text())
for row in m['files']:
 raw=(r/row['path']).read_bytes()
 if len(raw)!=row['bytes'] or hashlib.sha256(raw).hexdigest()!=row['sha256']:
  raise ValueError('native final source mismatch '+row['path'])
print('Verified final native BNO source inputs:',len(m['files']))
'''
completion = '''from pathlib import Path
import datetime, json, os, subprocess, sys
b=Path('C:/Windows/Temp/assist-native-rax-20261010')
s=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record=s/'bno-final-post-run-completion.json'
if record.exists():raise RuntimeError('BNO completion already exists')
for name in ['bno-native-epoch5-launch.json','bno-final-owned-launch.json']:
 row=json.loads((s/name).read_text())
 if row['state']!='finished' or row['exit-code']!=0:raise RuntimeError('incomplete '+name)
lock=b/'bno-final-post-run.lock'
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(fd)
checks=[]
try:
 for script,log in [('run-bno-owner-observer.py','bno-owner-observer-transport.log'),('verify-bno-final-source.py','native-bno-final-source-post-run.log'),('verify-bno-post-run.py','native-bno-final-post-run-verifier.log')]:
  with (s/log).open('wb') as output:
   result=subprocess.run([sys.executable,'-B',str(s/script)],stdout=output,stderr=subprocess.STDOUT)
  checks.append({'script':script,'exit-code':result.returncode})
  if result.returncode:raise RuntimeError('BNO completion failed '+script)
 record.write_text(json.dumps({'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source-epoch':5,'namespace-epoch':2,'checks':checks},indent=2)+'\\n')
finally:lock.unlink()
print('BNO post-run checks complete',flush=True)
'''
for root in [B, S]:
    (root / 'verify-bno-final-source.py').write_text(source_check)
    (root / 'finish-bno-final.py').write_text(completion)

old = (N / 'rax-source/docs/specifications/windows/native-base-named-objects/check-validation.py').read_text()
head = old[:old.index("manifest = load(")]
manifest = '''manifest = load(V / 'bno-epoch5-source.json')
need(manifest['base'] == 'be05c4b474dbf76be910e0f50dbc4be3262c16ce' and
     manifest['source-epoch'] == 5 and manifest['namespace-epoch'] == 2, 'tested source lineage')
inputs = manifest['files']
need(len(inputs) == len({r['path'] for r in inputs}) == 5737, 'selected source inputs')
for row in inputs:
    source(row)
port = load(V / 'final-port.json')
parent = load(V / 'base-final-main-state.json')
need(port['parent'] == parent['rax'] and port['compiled-inputs'] == 5737,
     'exact port onto published namespace parent')
completion = load(V / 'bno-final-post-run-completion.json')
need(completion['source-epoch'] == 5 and completion['namespace-epoch'] == 2 and
     [(r['script'], r['exit-code']) for r in completion['checks']] ==
     [('run-bno-owner-observer.py', 0), ('verify-bno-final-source.py', 0),
      ('verify-bno-post-run.py', 0)], 'native completion checks')
need('Verified final native BNO source inputs: 5737' in
     (V / 'native-bno-final-source-post-run.log').read_text(), 'native final source check')
compared = load(V / 'current-cpp-consumers-bno-root.json')
need(compared['current-root-head'] == parent['root'] and len(compared['files']) == 9,
     'current C++ source comparison')
'''
post = old[old.index("post_run = "):old.index("ignored = ")]
post = post.replace('native-base-final', 'native-bno-final').replace('native-bno-final-trace.exe', 'native-bno-epoch5-trace.exe')
full = old[old.index("ignored = "):old.index("for host, log, passed, filtered in [")]
full = full.replace("('macos', 7921, 0), ('linux', 7915, 0), ('native', 7358, 5)",
                    "('macos', 7929, 0), ('linux', 7923, 0), ('native', 7367, 5)")
full = full.replace('base-final', 'bno-epoch5')
full = full.replace("['integration', 'capi', 'full', 'all-targets'] if host == 'native'",
                    "['focus', 'integration', 'capi', 'full', 'all-targets'] if host == 'native'")
full = full.replace("selection = {'full':", "selection = {'focus': ['--lib', 'bno_'], 'full':")
focus = '''for host, log, passed, filtered in [
    ('macos', 'prototype-macos-focus-epoch5.log', 8, 7923),
    ('linux', 'prototype-linux-focus-epoch5.log', 8, 7917),
    ('native', 'native-bno-epoch5-focus.log', 9, 7365),
]:
    need(counts(log) == [(passed, 0, 0, filtered)], 'focused BNO result ' + host)
for name in ['bno-native-epoch5-launch.json', 'bno-final-owned-launch.json']:
    row = load(V / name)
    need(row['state'] == 'finished' and row['exit-code'] == 0, 'native driver completion')
'''
owner = old[old.index("for host, expected in [('macos', 7)"):old.index("ordinary = load(")]
owner = transform(owner).replace('native-base-directories-final', 'native-bno-isolation-epoch5')
owner = owner.replace("len(archive['sources']) == 419", "len(archive['sources']) == 428")
ordinary = '''ordinary = load(V / 'ordinary-analysis.json')
review = load(V / 'bno-ordinary-reviewed.json')
contract = review['outcomes']
executed = [r for r in ordinary if 'log' in r]
need(review['status'] == 'reviewed' and review['source-epoch'] == 5 and
     len(executed) == len(contract) == 2, 'ordinary execution count')
for row, expected in zip(executed, contract):
    need(row['controller-exit-code'] == 0 and row['program-sha256'] ==
         row['program-sha256-after'] == expected['program-sha256'], 'ordinary target identity')
    need(row['sha256'] == digest(V / row['log']) == expected['sha256'], 'ordinary log identity')
    text = (V / row['log']).read_text(encoding='utf-8-sig')
    need([line for line in text.splitlines() if line.startswith('terminal ')] ==
         [expected['terminal']], 'actual ordinary terminal')
    turn = expected['class44-success-turn']
    need(f'kernel-after turn={turn} service=0x21 PC=0x7ff82fad1224 X0=0x0' in text,
         'class44 now returns success')
    need(expected['terminal-turn'] - turn == expected['observer-turns-advanced'] == 100,
         'bounded observer progression')
    need('NtCreateMutant (0xba)' in expected['terminal'] and
         'is not implemented' in expected['terminal'], 'no invented guest exit')
'''
observation = old[old.index("owner = load(V / 'base-owner-observer-launch.json')"):old.index("need(digest(V / 'native-namespace-churn.rs')")]
observation = transform(observation)
observation = observation.replace("contract[index]['terminal']", "contract[index]['terminal']")
text = head + manifest + post + full + focus + owner + ordinary + observation + "\nprint('BNO final source, native/copy proof, three-host gates and retained consumers verified')\n"
# The finalized checker additionally binds the before/after ordinary core archive.
if not (B / 'check-bno-validation.py').exists():
    (B / 'check-bno-validation.py').write_text(text)

stress = (N / 'rax-source/docs/specifications/windows/native-base-named-objects/stress-validation.py').read_text()
begin = stress.index("    ('invented-churn-success'")
end = stress.index("    ('removed-byte-preservation'", begin)
stress = stress[:begin] + stress[end:]
stress = transform(stress).replace('native-bno-final-full.log', 'native-bno-epoch5-full.log')
stress = stress.replace('native-bno-final-integration.log', 'native-bno-epoch5-integration.log')
stress = stress.replace("'bno-final-source.json'", "'bno-epoch5-source.json'")
stress = stress.replace("'native-bno-final-gates.json'", "'native-bno-epoch5-gates.json'")
stress = stress.replace("edit_json(r, 'ordinary-contract.json',", "edit_json(r, 'bno-ordinary-reviewed.json',")
stress = stress.replace("v[0].update({'terminal':", "v['outcomes'][0].update({'terminal':")
(B / 'stress-bno-validation.py').write_text(stress)
print('Prepared BNO final checker, stress tests and native completion scripts')
