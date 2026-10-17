from pathlib import Path
import hashlib,json,os,subprocess,sys,time
b=Path(__file__).resolve().parent;host=sys.argv[1];source=b/'rax-source-final'
env=os.environ.copy();env['CARGO_BUILD_JOBS']='2';env['RUSTUP_TOOLCHAIN']='1.95.0';env['CARGO_TARGET_DIR']=str(b/'target-final-macos')
prefix=[] if host=='macos' else ['docker','run','--rm','--platform','linux/amd64','-v','/Users/int/hexrays/kvasir:/work:ro','-v',str(source)+':/work/vendor/rax:ro','-v','rax-cargo-registry:/usr/local/cargo/registry','-v','rax-cargo-git:/usr/local/cargo/git','-v','rax-user-target:/target','-e','CARGO_TARGET_DIR=/target/image-extension-owner-final','-e','CARGO_BUILD_JOBS=2','-e','RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu','-w','/work/vendor/rax','assist-rax-validation-rustfmt:1.95-bullseye-20261015']
rows=[]
for name,command in [('focus',['test','--lib','image_extension']),('full',['test','--lib']),('capi',['test','-p','rax-capi']),('integration',['test','--test','user_windows','--test','user_windows_memory']),('all-targets',['check','--all-targets'])]:
 argv=prefix+['cargo',command[0],'--locked','--offline','--no-default-features',*command[1:],*(['--','--nocapture','--test-threads=1'] if command[0]=='test' else [])];p=b/(host+'-image-extension-final-'+name+'.log');t=time.monotonic()
 with p.open('wb') as output:result=subprocess.run(argv,cwd=source,env=env,stdout=output,stderr=subprocess.STDOUT)
 rows.append({'gate':name,'argv':argv,'exit-code':result.returncode,'seconds':time.monotonic()-t,'log':p.name,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()});(b/(host+'-wnf-system-epoch2-gates.json')).write_text(json.dumps(rows,indent=2)+'\n');print(host,name,result.returncode,flush=True)
 if result.returncode and b'could not compile' in p.read_bytes():break
sys.exit(int(any(r['exit-code'] for r in rows)))
