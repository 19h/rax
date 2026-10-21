from pathlib import Path
import datetime,hashlib,json,os,subprocess,time
Q=Path(__file__).parent;W=Path('/Users/int/.assist-native-alpc-csr-20261021');P=Path('/Users/int/.assist-native-process-mitigation-20261021');F=Q/'alpc-cross-candidate2-retry1.json';assert not F.exists();m=Q/'alpc-candidate2-source.json';assert hashlib.sha256(m.read_bytes()).hexdigest()=='662497516020160fb00c21e0b56eb2ee941c5d08ac125ba1c0321973db4db2f0';inputs=json.loads(m.read_text())['files'];state={'state':'running','pid':os.getpid(),'source-ledger-sha256':hashlib.sha256(m.read_bytes()).hexdigest(),'targets':[]}
def save():F.write_text(json.dumps(state,indent=2)+'\n')
def verify():
 for r in inputs:
  p=W/r['path'];raw=os.readlink(p).encode() if r['kind']=='symlink-text' else p.read_bytes();assert (len(raw),hashlib.sha256(raw).hexdigest())==(r['bytes'],r['sha256']),r['path']
save()
for target,short in [('aarch64-pc-windows-msvc','aarch64'),('x86_64-pc-windows-msvc','x64')]:
 dest=W/'target'/('cross-'+short+'-candidate2-retry1');assert not dest.exists();subprocess.run(['cp','-cR',str(P/'target'/('cross-'+short)),str(dest)],check=True);env=dict(os.environ,CARGO_TARGET_DIR=str(dest),CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0');verify();clean=Q/f'alpc-cross-{short}-candidate2-retry1-clean.log'
 with clean.open('wb') as out:subprocess.run(['cargo','+1.95.0','clean','-p','rax'],cwd=W,env=env,stdout=out,stderr=subprocess.STDOUT,check=True)
 log=Q/f'alpc-cross-{short}-candidate2-retry1.log';args=['cargo','+1.95.0','xwin','check','--locked','--offline','--no-default-features','--lib','--target',target]
 with log.open('wb') as out:
  child=subprocess.Popen(args,cwd=W,env=env,stdout=out,stderr=subprocess.STDOUT);state.update({'target':target,'child-pid':child.pid});save();code=child.wait()
 verify();state['targets'].append({'target':target,'argv':args,'exit-code':code,'log':log.name,'bytes':log.stat().st_size,'sha256':hashlib.sha256(log.read_bytes()).hexdigest(),'clean-log':clean.name,'primary-package-cleaned':True});save();assert code==0,target
state.update(state='finished',**{'exit-code':0,'completed-utc':datetime.datetime.now(datetime.timezone.utc).isoformat()});save();print('cross-pass',len(state['targets']))
