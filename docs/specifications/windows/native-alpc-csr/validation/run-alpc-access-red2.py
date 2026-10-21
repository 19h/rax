from pathlib import Path
import datetime,hashlib,json,os,shutil,subprocess
Q=Path(__file__).parent;W=Path('/Users/int/.assist-native-alpc-csr-20261021');BASE=Q/'source-candidate1';R=Q/'red-source-v2';F=Q/'alpc-access-length-red2.json';assert not R.exists() and not F.exists();R.mkdir();base=json.loads((Q/'alpc-candidate1-source.json').read_text());parent='src/user/windows/process/sched/tests/services_tests/alpc_tests.rs';extra='src/user/windows/process/sched/tests/services_tests/alpc_tests/limits.rs';native=[f'docs/specifications/windows/native-alpc-csr/native/native-alpc-private-limits-v9-four-views-{p}.log' for p in ['arm64','x64','x86','x86-laa']]
for row in base['files']:
 if row['path']==parent:continue
 p=R/row['path'];p.parent.mkdir(parents=True,exist_ok=True)
 if row['kind']=='symlink-text':p.symlink_to(os.readlink(BASE/row['path']))
 else:os.link(BASE/row['path'],p)
text=(BASE/parent).read_text();needle='mod replay;';assert needle in text;text=text.replace(needle,needle+'\n\n#[path = "alpc_tests/limits.rs"]\nmod limits;',1);(R/parent).write_text(text)
for name in [extra,*native]:p=R/name;p.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(W/name,p)
subprocess.run(['rustfmt','+1.95.0','--edition','2024','--config','skip_children=true',str(R/parent)],check=True)
inputs=[]
for name in sorted({x['path'] for x in base['files']}|{extra,*native}):
 p=R/name;raw=os.readlink(p).encode() if p.is_symlink() else p.read_bytes();inputs.append({'path':name,'kind':'symlink-text' if p.is_symlink() else 'file','bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
def verify():
 for row in inputs:
  p=R/row['path'];raw=os.readlink(p).encode() if p.is_symlink() else p.read_bytes();assert (len(raw),hashlib.sha256(raw).hexdigest())==(row['bytes'],row['sha256']),row['path']
manifest=Q/'alpc-access-length-red2-source.json';manifest.write_text(json.dumps({'base-source':'alpc-candidate1-source.json','files':inputs,'overrides':[parent,extra,*native],'scope':'Immutable pre-fix candidate1 kernel plus unchanged final limits regressions/native logs. Independent override files; all others hardlink immutable retained source, never mutable worktree.'},indent=2)+'\n');target=W/'target/access-red2';assert not target.exists();subprocess.run(['cp','-cR',str(W/'target/client-data-v1'),str(target)],check=True);env=dict(os.environ,CARGO_TARGET_DIR=str(target),CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0',TMPDIR=str(W/'target/tmp'));verify()
with (Q/'alpc-access-length-red2-clean.log').open('wb') as out:subprocess.run(['cargo','+1.95.0','clean','-p','rax'],cwd=R,env=env,stdout=out,stderr=subprocess.STDOUT,check=True)
log=Q/'alpc-access-length-red2.log';argv=['cargo','+1.95.0','test','--locked','--offline','--no-default-features','--lib','services_tests::alpc_tests::limits','--','--test-threads=1'];state={'state':'running','pid':os.getpid(),'source-manifest-sha256':hashlib.sha256(manifest.read_bytes()).hexdigest(),'source-inputs':len(inputs),'argv':argv};F.write_text(json.dumps(state,indent=2)+'\n')
with log.open('wb') as out:
 child=subprocess.Popen(argv,cwd=R,env=env,stdout=out,stderr=subprocess.STDOUT);state['child-pid']=child.pid;F.write_text(json.dumps(state,indent=2)+'\n');code=child.wait()
verify();raw=log.read_bytes();text=raw.decode();assert code==101 and '0 passed; 2 failed;' in text;state.update(state='finished',**{'exit-code':0,'observed-test-exit':code,'log':log.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'completed-utc':datetime.datetime.now(datetime.timezone.utc).isoformat()});F.write_text(json.dumps(state,indent=2)+'\n');print('pre-fix kernel rejected by both final regressions')
