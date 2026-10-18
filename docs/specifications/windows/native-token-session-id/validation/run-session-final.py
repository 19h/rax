from pathlib import Path
import hashlib,json,os,subprocess,sys,time
b=Path(__file__).resolve().parent;host=sys.argv[1];source=b/'rax-source'
env=dict(os.environ,CARGO_BUILD_JOBS='2',RUSTUP_TOOLCHAIN='1.95.0',CARGO_TARGET_DIR=str(b.parent/'native-system-time-20261017/target-macos'))
prefix=[] if host=='macos' else ['docker','run','--rm','--platform','linux/amd64','-v','/Users/int/hexrays/kvasir:/work:ro','-v',str(source)+':/work/vendor/rax:ro','-v','rax-cargo-registry:/usr/local/cargo/registry','-v','rax-cargo-git:/usr/local/cargo/git','-v','rax-user-target:/target','-e','CARGO_TARGET_DIR=/target/system-time-final','-e','CARGO_BUILD_JOBS=2','-e','RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu','-w','/work/vendor/rax','assist-rax-validation-rustfmt:1.95-bullseye-20261015']
rows=[]
for name,command in [('full',['test','--lib']),('capi',['test','-p','rax-capi']),('integration',['test','--test','user_windows','--test','user_windows_memory']),('all-targets',['check','--all-targets'])]:
 a=prefix+['cargo',command[0],'--locked','--offline','--no-default-features',*command[1:],*(['--','--nocapture','--test-threads=1'] if command[0]=='test' else [])]
 p=b/(host+'-session-final-'+name+'.log');start=time.time()
 with p.open('wb')as out:r=subprocess.run(a,cwd=source,env=env,stdout=out,stderr=subprocess.STDOUT)
 rows.append({'gate':name,'argv':a,'exit-code':r.returncode,'seconds':time.time()-start,'log':p.name,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()});(b/(host+'-session-final-gates.json')).write_text(json.dumps(rows,indent=2)+'\n');print(host,name,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
