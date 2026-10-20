from pathlib import Path
import datetime,hashlib,json,os,shutil,subprocess,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');R=B/'probe-src-file-attributes-v5';record=S/'native-file-attributes-v5-launch.json';lock=B/'native-file-attributes-v5.lock';SHA='12f59a5fd3117bd8c936fc9f9becfbf610e4023c4c6cd4e3e39e6fe56f009654'
assert not R.exists() and not record.exists()
fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.close(fd)
r={'state':'starting','pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'scope':'Independent native basic/full file-attribute capture and fault-order oracle for private fixtures and query-only installedVERSION.dll across four ABI profiles. All outputs remain under this probe source workspace.','steps':[],'outcomes':[]}
def save():record.write_text(json.dumps(r,indent=2)+'\n')
def identity(path):
 raw=path.read_bytes();return {'path':str(path),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
def run(name,args,cwd,env):
 log=S/('native-file-attributes-v5-'+name+'.log')
 with log.open('wb')as out:
  child=subprocess.Popen([str(x)for x in args],cwd=cwd,env=env,stdout=out,stderr=subprocess.STDOUT);r.update(state='running',step=name,child=child.pid);save();code=child.wait()
 raw=log.read_bytes();r['steps'].append({'name':name,'argv':[str(x)for x in args],'exit-code':code,'log':log.name,'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw)});save();return code,log
modules=[Path('C:/Windows')/part/dll for dll in ['ntdll.dll','KernelBase.dll','kernel32.dll']for part in ['System32','SysWOW64']]
save()
try:
 R.mkdir();(R/'target/tmp').mkdir(parents=True);source=R/'native-file-attributes-v5.cpp';shutil.copy2(S/source.name,source);assert identity(source)['sha256']==SHA;r['source-before']=identity(source)
 environment_path=S/'mutant-native-compiler-env-epoch4.json';base=json.loads(environment_path.read_text(encoding='utf-8-sig'));r['compiler-environment']=identity(environment_path);r['runtime-modules-before']=[identity(p)for p in modules];save()
 for profile in ['arm64','x64','x86','x86-laa']:
  arch='x86'if profile=='x86-laa'else profile;out=R/'target'/profile;out.mkdir();env=dict(os.environ)
  for name in ['CL','_CL_','LINK','_LINK_']:env.pop(name,None)
  env.update(base)
  for name in ['LIB','LIBPATH']:
   env[name]=';'.join(str(Path(x).with_name(arch))if x and Path(x).name.lower()=='arm64'else x for x in base[name].split(';'))
  compiler=Path(base['VCToolsInstallDir'])/'bin/Hostarm64'/arch/'cl.exe';assert compiler.is_file(),str(compiler)
  env.update(PATH=str(compiler.parent)+';'+base['PATH'],TMP=str(R/'target/tmp'),TEMP=str(R/'target/tmp'),VSCMD_ARG_TGT_ARCH=arch,PYTHONDONTWRITEBYTECODE='1')
  exe=out/'probe.exe';r.setdefault('selected-compilers',[]).append({'profile':profile,**identity(compiler)});save()
  args=[compiler,'/nologo','/std:c++20','/EHsc','/MT','/GS','/Od','/we4473','/we4474','/we4477',source,'/link','/SUBSYSTEM:CONSOLE,10.0','/OUT:'+str(exe)]
  if profile=='x86-laa':args.append('/LARGEADDRESSAWARE')
  code,_=run(profile+'-build',args,out,env);assert code==0,'compile '+profile
  before_args=[B/'python/python.exe','-B',S/'inspect-system-time-v2-pe.py',exe,profile]
  code,pe_before=run(profile+'-before',before_args,out,env);assert code==0
  before=identity(exe);code,log=run(profile,[exe,profile],out,env);after=identity(exe)
  r['outcomes'].append({'profile':profile,'exit-code':code,'log':log.name,'executable-before':before,'executable-after':after});save();assert before==after
  code,_=run(profile+'-after',before_args,out,env);assert code==0
  assert identity(source)==r['source-before']
  assert r['outcomes'][-1]['exit-code']==0,'native producer '+profile
 r['runtime-modules-after']=[identity(p)for p in modules];assert r['runtime-modules-before']==r['runtime-modules-after'];r['source-after']=identity(source);r.update(state='finished',**{'exit-code':0})
except BaseException:r.update(state='failed',**{'exit-code':1,'error':traceback.format_exc()})
finally:r['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(r),flush=True);raise SystemExit(r['exit-code'])
