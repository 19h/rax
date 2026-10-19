from pathlib import Path
import hashlib,json,shutil
N=Path(__file__).resolve().parent;S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
source=(N/'wait-owner-observer.rs').read_bytes()
for root in [N,S]:(root/'wait-owner-epoch4.rs').write_bytes(source)
ps=(N/'wait-owner-observer-build.ps1').read_text().replace('native-wait-final-owning-archive-identities-b','native-wait-epoch4-final-owning-archive-identities').replace('wait-owner-observer','wait-owner-epoch4').replace('wait_owner_observer','wait_owner_epoch4')
driver=(N/'run-wait-owner-observer.py').read_text().replace('wait-final-owned-b-launch','wait-native-owning-epoch4').replace('native-wait-final-owning-archive-identities-b','native-wait-epoch4-final-owning-archive-identities').replace('src-wait-epoch3','src-wait-epoch4').replace('wait-owner-observer','wait-owner-epoch4')
for root in [N,S]:
 (root/'wait-owner-epoch4-build.ps1').write_text(ps);(root/'run-wait-owner-epoch4.py').write_text(driver)
(N/'wait-owner-epoch4-source.json').write_text(json.dumps({'source':'wait-owner-epoch4.rs','sha256':hashlib.sha256(source).hexdigest(),'bytes':len(source),'scope':'Same read-only observer as epoch3, linked to the actual final epoch4 retained-owner core. Unchanged programs; no guest PC/status patch or syscall suppression.'},indent=2)+'\n')
print('Prepared final epoch4 observer')
