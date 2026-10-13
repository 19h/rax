from pathlib import Path
import json,subprocess
base=Path(__file__).parent
root='/Users/int/hexrays/kvasir'
mount=['docker','run','--rm','--platform','linux/amd64','-v',root+':/work:ro','-v','rax-cargo-registry:/usr/local/cargo/registry','-v','rax-cargo-git:/usr/local/cargo/git','-v','rax-user-target:/target','-e','CARGO_TARGET_DIR=/target','-e','RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu','-w','/work/vendor/rax','rust:1.95-bullseye']
tests=['user::linux::tests::uring::timeout::a_multishot_timeout_reports_each_expiry','user::linux::tests::uring::timeout::a_timeout_is_removed_or_updated','user::readiness::tests::finite_empty_and_expired_waits']
results=[]
for i,test in enumerate(tests):
 command=mount+['cargo','test','--locked','--offline','--no-default-features','--lib',test,'--','--exact','--nocapture','--test-threads=1']
 with (base/('linux-isolated-'+str(i)+'.log')).open('w') as log:r=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT)
 results.append({'gate':test,'exit-code':r.returncode,'command':command});(base/'linux-failure-audit.json').write_text(json.dumps(results,indent=2)+'\n');print(test,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
command=mount+['cargo','test','--locked','--offline','--no-default-features','--lib','--','--nocapture','--test-threads=1']
with (base/'linux-full-serial.log').open('w') as log:r=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT)
results.append({'gate':'unfiltered-full-serial','exit-code':r.returncode,'command':command});(base/'linux-failure-audit.json').write_text(json.dumps(results,indent=2)+'\n');print('unfiltered-full-serial',r.returncode,flush=True)
raise SystemExit(r.returncode)
