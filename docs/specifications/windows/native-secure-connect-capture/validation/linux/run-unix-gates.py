from pathlib import Path
import subprocess,json,sys,time,hashlib,os
c=Path(__file__).parent;r=c.parent/'rax-source';host=sys.argv[1];record=json.loads((c.parent/'secure-capture-sources.json').read_text())
for f in record['files']:
 b=(r/f['path']).read_bytes();assert len(b)==f['bytes'] and hashlib.sha256(b).hexdigest()==f['sha256'],f['path']
pre=[]
if host=='linux':pre=['docker','run','--rm','--platform','linux/amd64','-v','/Users/int/hexrays/kvasir:/work:ro','-v',str(r)+':/work/vendor/rax:ro','-v','rax-cargo-registry:/usr/local/cargo/registry','-v','rax-cargo-git:/usr/local/cargo/git','-v','rax-user-target:/target','-e','CARGO_BUILD_JOBS=2','-e','CARGO_TARGET_DIR=/target','-e','RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu','-w','/work/vendor/rax','assist-rax-validation-rustfmt:1.95-bullseye-20261015']
steps=[('focused',['test','--lib','secure_connect']),('lib',['test','--lib']),('capi',['test','-p','rax-capi']),('all-targets',['check','--all-targets']),('windows',['test','--test','user_windows','--test','user_windows_memory']),('build-user',['build','--bin','rax-user'])]
rows=[]
for name,args in steps:
 cmd=pre+['cargo']+args+['--locked','--offline','--no-default-features']
 if args[0]=='test':cmd+=['--','--test-threads=1']
 start=time.time();log=c/(host+'-'+name+'.log')
 with log.open('wb') as out:code=subprocess.run(cmd,cwd=r,env=dict(os.environ,CARGO_TARGET_DIR='/Users/int/Library/Caches/assist-process-emulation/darwin-reclaim-pressure-20261015/target-pinned',CARGO_BUILD_JOBS='2',RUSTUP_TOOLCHAIN='1.95.0'),stdout=out,stderr=subprocess.STDOUT).returncode
 rows.append({'gate':name,'command':cmd,'exit-code':code,'seconds':time.time()-start,'log':log.name,'sha256':hashlib.sha256(log.read_bytes()).hexdigest(),'source-commit':record['base']});(c/(host+'-gates.json')).write_text(json.dumps(rows,indent=2)+'\n');print(host,name,'exit',code,flush=True)
