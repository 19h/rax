from pathlib import Path
import hashlib,json,shutil
n=Path(__file__).resolve().parent;s=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
source=n/'wait-owner-observer.rs';sha=hashlib.sha256(source.read_bytes()).hexdigest();shutil.copy2(source,s/source.name)
ps=(n/'wait-baseline-observer-build.ps1').read_text().replace('native-mutant-final-owning-archive-identities','native-wait-final-owning-archive-identities').replace('target-io-completion-owned','target-wait-owned').replace('wait-baseline-observer','wait-owner-observer').replace('wait_baseline_observer','wait_owner_observer')
driver=(n/'run-wait-baseline-observer.py').read_text().replace('wait-baseline-observer','wait-owner-observer').replace('mutant-final-owned','wait-final-owned').replace('native-mutant-final-owning-archive-identities','native-wait-final-owning-archive-identities').replace('src-mutant-epoch4','src-wait-epoch3').replace('39ab455a2077c3a45535f379732d4f48977c39b3ef66fd32e3d7ce7dab317b85',sha)
for root in [n,s]:
 (root/'wait-owner-observer-build.ps1').write_text(ps)
 (root/'run-wait-owner-observer.py').write_text(driver)
(n/'wait-owner-observer-source.json').write_text(json.dumps({'source':source.name,'sha256':sha,'bytes':source.stat().st_size,'scope':'Read-only observations of unchanged ordinary programs, linked to the actual validated retained-owner core archive. No guest PC/status patches or syscall suppression. At most3000000 scheduler turns per program.'},indent=2)+'\n')
print(sha)
