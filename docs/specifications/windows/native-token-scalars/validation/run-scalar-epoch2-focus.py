from pathlib import Path
import datetime,hashlib,json,os,subprocess,sys,time
b=Path(__file__).resolve().parent;host=sys.argv[1];source=b/'rax-source'
env=dict(os.environ,RUSTUP_TOOLCHAIN='1.95.0',CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(b.parent/'native-system-time-20261017/target-macos'))
prefix=[] if host=='macos' else ['docker','run','--rm','--platform','linux/amd64','-v','/Users/int/hexrays/kvasir:/work:ro','-v',str(source)+':/work/vendor/rax:ro','-v','rax-cargo-registry:/usr/local/cargo/registry','-v','rax-cargo-git:/usr/local/cargo/git','-v','rax-user-target:/target','-e','CARGO_TARGET_DIR=/target/system-time-final','-e','CARGO_BUILD_JOBS=2','-e','RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu','-w','/work/vendor/rax','assist-rax-validation-rustfmt:1.95-bullseye-20261015']
args=prefix+['cargo','test','--locked','--offline','--no-default-features','--lib','token','--','--nocapture','--test-threads=1']
log=b/(host+'-scalar-epoch2-focus.log');start=time.time()
with log.open('wb')as f:code=subprocess.run(args,cwd=source,env=env,stdout=f,stderr=subprocess.STDOUT).returncode
(b/(host+'-scalar-epoch2-focus.json')).write_text(json.dumps({'argv':args,'exit-code':code,'seconds':time.time()-start,'sha256':hashlib.sha256(log.read_bytes()).hexdigest()},indent=2)+'\n');print(host,'focus',code,flush=True)
