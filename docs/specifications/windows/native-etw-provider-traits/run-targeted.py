from pathlib import Path
import json,os,subprocess,sys
base=Path(__file__).parent;root='/Users/int/hexrays/kvasir';host=sys.argv[1]
command=['cargo','test','--locked','--offline','--no-default-features','--lib','etw_provider_traits','--','--nocapture']
if host=='macos':os.environ['CARGO_TARGET_DIR']='/Users/int/Library/Caches/assist-process-emulation/io-completion-create-20261011/red-target'
else:command=['docker','run','--rm','--platform','linux/amd64','-v',root+':/work:ro','-v','rax-cargo-registry:/usr/local/cargo/registry','-v','rax-cargo-git:/usr/local/cargo/git','-v','rax-user-target:/target','-e','CARGO_TARGET_DIR=/target','-e','RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu','-w','/work/vendor/rax','rust:1.95-bullseye']+command
with (base/(host+'-targeted.log')).open('w') as log:r=subprocess.run(command,cwd=root+'/vendor/rax',stdout=log,stderr=subprocess.STDOUT)
(base/(host+'-targeted-gate.json')).write_text(json.dumps({'command':command,'exit-code':r.returncode},indent=2)+'\n');print(host,r.returncode,flush=True);raise SystemExit(r.returncode)
