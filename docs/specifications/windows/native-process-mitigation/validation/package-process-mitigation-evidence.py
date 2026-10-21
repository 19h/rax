"""Publish declared proof inputs with explicit private compiler projections."""
from pathlib import Path
import hashlib,json
Q=Path(__file__).parent
S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
V=Path('/Users/int/.assist-native-process-mitigation-20261021/docs/specifications/windows/native-process-mitigation/validation')
def identity(raw):return {'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
def locate(name):
 candidates=[base/name for base in [Q,S] if (base/name).exists()]
 if not candidates:raise FileNotFoundError(name)
 raw=candidates[0].read_bytes()
 for p in candidates[1:]:assert p.read_bytes()==raw,'ambiguous input '+name
 return raw
receipts=['process-mitigation-macos-candidate2-gates.json','process-mitigation-linux-candidate2-gates.json','process-mitigation-candidate2-windows-retry2.json','macos-process-mitigation-candidate2-owning-gates.json','linux-process-mitigation-candidate2-owning-gates.json','process-mitigation-native-owning-candidate2-retry2.json','process-mitigation-native-ordinary-candidate2-retry2.json','process-mitigation-owning-capi-candidate2-retry2.json']
failed=['process-mitigation-macos-candidate1-gates.json','process-mitigation-candidate1-windows.json','process-mitigation-candidate2-windows.json','process-mitigation-candidate2-windows-retry1.json']
names=set(receipts+failed)
for name in receipts+failed:
 row=json.loads(locate(name))
 assert row['state']==('failed' if name in failed else 'finished'),name
 names.update(step['log'] for step in row['steps'])
names.update(['process-mitigation-candidate1-source.json','process-mitigation-candidate1-overrides.tar.gz','process-mitigation-candidate2-source.json','process-mitigation-candidate2-overrides.tar.gz','process-mitigation-owning-source-hashes-candidate2.json','process-mitigation-observer-source-candidate2.json','process-mitigation-owning-capi-source.json','process-mitigation-ordinary-owner-candidate2.rs','process-mitigation-owning-capi.cpp','process-mitigation-red1-source.json','process-mitigation-red1.json','process-mitigation-red1.log','process-mitigation-red1-clean.log','process-mitigation-red-tests.rs','process-mitigation-red-prefix-binding.json','process-mitigation-native-proof-stress-v1.json','stress-process-mitigation-native-proof-v1.py','process-mitigation-cross-final.json','process-mitigation-clippy-linux-canonical.log','process-mitigation-rustfmt-final.log','process-mitigation-retained-owning-candidate2.json','process-mitigation-final-native-audit-v2.json','audit-process-mitigation-final-native-v2.py','macos-process-mitigation-candidate2-linkage.json','process-mitigation-owning-retarget-macos.json','process-mitigation-owning-retarget-linux.json','process-mitigation-linux-candidate1-clean.log','process-mitigation-linux-candidate2-clean.log','process-mitigation-macos-candidate1-clean.log','process-mitigation-macos-candidate2-clean.log','process-mitigation-old-symbol-cleanup-v1.json','process-mitigation-old-static-cleanup-v1.json','process-mitigation-pdb-failure-disk-v1.json','process-mitigation-superseded-waiters-v1.json','process-mitigation-superseded-waiters-v2.json','native-process-mitigation-candidate2-pre-cpp-archives.json','native-process-mitigation-candidate2-final-owning-archive-identities.json'])
for host in ['macos','linux']:
 for suffix in ['archives-before.json','archives-after.json','owning-commands.json']:names.add(host+'-process-mitigation-candidate2-'+suffix)
for host in ['macos','linux','windows']:names.add('process-mitigation-native-proof-'+host+'-v1.json')
for arch in ['aarch64','x64']:
 names.add('process-mitigation-candidate2-cross-'+arch+'.log');names.add('process-mitigation-cross-'+arch+'-clean.log')
for name in ['run-process-mitigation-candidate1.py','run-process-mitigation-candidate2.py','run-process-mitigation-candidate2-retry1.py','run-process-mitigation-candidate2-retry2.py','run-process-mitigation-unix-candidate1.py','run-process-mitigation-unix-candidate2.py','run-process-mitigation-owning-candidate2.py','run-process-mitigation-native-owning-candidate2-retry2.py','run-process-mitigation-native-ordinary-candidate2-retry2.py','run-process-mitigation-owning-capi-candidate2-retry2.py','collect-process-mitigation-archives-candidate2.py','process-mitigation-owned-cpp-build-candidate2.py','owning-linux-cpp-build-process-mitigation-candidate2.py','retain-process-mitigation-artifacts-candidate2.py','cancel-process-mitigation-superseded-waiters-v1.py','cancel-process-mitigation-superseded-waiters-v2.py','cleanup-process-mitigation-old-symbols-v1.py','cleanup-process-mitigation-old-static-v1.py','inspect-process-mitigation-pdb-failure-v1.py']:
 names.add(name)
for version in [1,2]:
 stem='native-alpc-frontier-diagnostic-v'+str(version)
 for suffix in ['.rs','-source.json','.json','-smoke.log','-whoami.log']:names.add(stem+suffix)
 names.add('run-'+stem+'.py')
# These are successful private owning builds, not source to publish upstream.
cargo={'macos-process-mitigation-candidate2-owning-prime-artifacts.log','linux-process-mitigation-candidate2-owning-shipping.log','linux-process-mitigation-candidate2-owning-artifacts.log','process-mitigation-native-owning-candidate2-retry2-artifacts-before.log','process-mitigation-native-owning-candidate2-retry2-artifacts-after.log'}
projections=[]
for name in sorted(names):
 raw=locate(name);published=raw
 if name in cargo:
  kept=[]
  for line in raw.decode(errors='replace').splitlines():
   try:row=json.loads(line)
   except ValueError:continue
   if row.get('reason')=='compiler-artifact':kept.append(line)
  assert kept,name
  published=('\n'.join(kept)+'\n').encode();projections.append({'path':name,'kind':'cargo-artifact-only','original':identity(raw),'published':identity(published)})
 if name=='macos-process-mitigation-candidate2-owning-build.log':
  published=b'Private Assist compiler transcript retained locally; owning receipt retains successful result and original byte identity.\n'
  projections.append({'path':name,'kind':'private-build-transcript-withheld','original':identity(raw),'published':identity(published)})
 (V/name).write_bytes(published)
(V/'public-projections.json').write_text(json.dumps({'files':projections},indent=2)+'\n')
print('declared',len(names),'projected',len(projections))
