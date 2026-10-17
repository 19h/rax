from pathlib import Path
import subprocess,json,hashlib,time,os,sys
p=Path(__file__).parent;r=p/'rax-source';host=sys.argv[1];out=p/(host+'-epoch1');out.mkdir(exist_ok=True)
rows={row['path']:row for row in json.loads((p/'epoch1-console-owner-code.json').read_text())['files']}
for name,row in rows.items():
 data=(r/name).read_bytes()
 if len(data)!=row['bytes'] or hashlib.sha256(data).hexdigest()!=row['sha256']:raise ValueError('frozeninput '+name)
pre=[]
if host=='linux':pre=['docker','run','--rm','--platform','linux/amd64','-v','/Users/int/hexrays/kvasir:/work:ro','-v',str(r)+':/work/vendor/rax:ro','-v','rax-cargo-registry:/usr/local/cargo/registry','-v','rax-cargo-git:/usr/local/cargo/git','-v','rax-user-target:/target','-e','CARGO_BUILD_JOBS=2','-e','CARGO_TARGET_DIR=/target','-e','RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu','-w','/work/vendor/rax','assist-rax-validation-rustfmt:1.95-bullseye-20261015']
results=[]
steps=[('focused',['test','--lib','native_console_owner']),('prior-services',['test','--lib','process_query_tests']),('lib',['test','--lib']),('capi',['test','-p','rax-capi']),('all-targets',['check','--all-targets']),('windows',['test','--test','user_windows','--test','user_windows_memory']),('build-user',['build','--bin','rax-user'])]
for gate,args in steps:
 cmd=pre+['cargo']+args+['--locked','--offline','--no-default-features']
 if args[0]=='test':cmd+=['--','--test-threads=1']
 log=out/(host+'-'+gate+'.log');start=time.time()
 env=dict(os.environ,CARGO_TARGET_DIR='/Users/int/Library/Caches/assist-process-emulation/csr-connection-model-20261015/target-macos',CARGO_BUILD_JOBS='2',RUSTUP_TOOLCHAIN='1.95.0')
 with log.open('wb') as f:code=subprocess.run(cmd,cwd=r,env=env,stdout=f,stderr=subprocess.STDOUT).returncode
 results.append({'gate':gate,'command':cmd,'exit-code':code,'seconds':time.time()-start,'log':log.name,'sha256':hashlib.sha256(log.read_bytes()).hexdigest(),'frozen-source-epoch':1,'verified-code-identities':list(rows.values())});(out/'gates.json').write_text(json.dumps(results,indent=2)+'\n');print(host,gate,code,flush=True)
