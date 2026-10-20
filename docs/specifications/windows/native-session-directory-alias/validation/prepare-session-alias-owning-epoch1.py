from pathlib import Path
import hashlib
import json
import os
import shutil

CACHE=Path(__file__).parent
PREVIOUS=CACHE.parent/'native-region-20261020'
ROOT=Path('/Users/int/.assist-session-alias-20261020')
SHARE=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
MANIFEST=CACHE/'session-alias-owning-source-hashes-epoch1.json'
assert not MANIFEST.exists()
for name in ['collect-region-archives-epoch1.py','region-owned-cpp-build-epoch1.py','owning-linux-cpp-build-region-epoch1.py','run-region-owning-epoch1.py','run-region-native-owning-epoch1.py']:
 text=(PREVIOUS/name).read_text().replace('region','session-alias')
 if name=='run-region-native-owning-epoch1.py':
  text=text.replace("(B/'src-namespace-epoch1').resolve()", "(B/'src-region-epoch1').resolve()")
  text=text.replace('Whole current-root ABI270','Whole current-root ABI272')
 target=CACHE/name.replace('region','session-alias');assert not target.exists();target.write_text(text)
 if name in ['collect-region-archives-epoch1.py','region-owned-cpp-build-epoch1.py','run-region-native-owning-epoch1.py']:shutil.copy2(target,SHARE/target.name)
for host in ['macos','linux']:
 name=host+'-region-epoch1-owning-commands.json';(CACHE/name.replace('region','session-alias')).write_text((PREVIOUS/name).read_text().replace('region','session-alias'))
value=json.loads((PREVIOUS/'region-owning-source-hashes-epoch1.json').read_text())
paths={row['path']for row in value['sources']}
parent={row['path']for row in json.loads((PREVIOUS/'region-source-epoch1.json').read_text())['files']}
frozen=json.loads((CACHE/'session-alias-source-epoch1.json').read_text())
paths.update(row['path']for row in frozen['files']if row['path']not in parent)
value['sources']=[{'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}for name in sorted(paths)for raw in [(ROOT/name).read_bytes()]]
value.update({'feature':'Verified current-session selected directory alias','epoch':1,'source-parent-rax':'6f2148297acc1e8106600306cc092b08207b80d2','scope':'Frozen RAX source and retained AssistABI184 actual archive/link/consumer boundary; currentABI272products remain separate.'})
for helper in value['helpers']:
 if helper['path']=='region-owned-cpp-build-epoch1.py':
  helper['path']=helper['path'].replace('region','session-alias');helper['native-path']=helper['native-path'].replace('region','session-alias');raw=(CACHE/helper['path']).read_bytes();helper.update({'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
MANIFEST.write_text(json.dumps(value,indent=2)+'\n');shutil.copy2(MANIFEST,SHARE/MANIFEST.name)
link=CACHE.parent/'wait-completion-packet-create-20261012/assist-root-current/vendor/rax';assert link.is_symlink()and link.resolve()==Path('/Users/int/.assist-region-20261020');before=os.readlink(link);link.unlink()
try:link.symlink_to(ROOT,target_is_directory=True)
except BaseException:link.symlink_to(before,target_is_directory=True);raise
(CACHE/'session-alias-macos-owning-link.json').write_text(json.dumps({'path':str(link),'before':before,'after':str(ROOT),'scope':'Only task-owned retained-consumer dependency symlink changes; no sharedcurrentcheckout or installedprofile mutation.'},indent=2)+'\n')
print('owning source inputs',len(value['sources']),flush=True)
