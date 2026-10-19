from pathlib import Path
import datetime,hashlib,json,os,shutil,subprocess,time,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154');ROOT=B/'assist';R=B/'src-continue-epoch1';F=S/'continue-owning-capi-b.json';assert not F.exists();state={'state':'preparing','pid':os.getpid(),'steps':[]}
def save():F.write_text(json.dumps(state,indent=2)+'\n')
def identity(path):
 h=hashlib.sha256();size=0
 with path.open('rb')as f:
  for raw in iter(lambda:f.read(1048576),b''):h.update(raw);size+=len(raw)
 return{'path':str(path),'bytes':size,'sha256':h.hexdigest()}
def run(name,args,env):
 log=S/('continue-owning-capi-b-'+name+'.log');started=time.monotonic()
 with log.open('wb')as output:
  child=subprocess.Popen(args,cwd=ROOT,env=env,stdout=output,stderr=subprocess.STDOUT);state.update({'state':'running','current-step':name,'child-pid':child.pid});save();code=child.wait()
 row={'name':name,'argv':args,'exit-code':code,'seconds':time.monotonic()-started,'log':log.name,**{k:v for k,v in identity(log).items()if k!='path'}};state['steps'].append(row);save();assert code==0,name
 return log
save()
try:
 ordinary=json.loads((S/'continue-native-ordinary-epoch1.json').read_text());assert ordinary['state']=='finished'and ordinary['exit-code']==0
 archives=json.loads((S/'native-continue-epoch1-final-owning-archive-identities.json').read_text());row=next(x for x in archives['archives']if x['path'].endswith('.lib'));archive=Path(row['path']);state['carrier-before']=identity(archive);assert(state['carrier-before']['bytes'],state['carrier-before']['sha256'])==(row['bytes'],row['sha256'])
 source=ROOT/'target/native-attributes/helpers/continue-owning-capi.cpp';shutil.copy2(S/source.name,source);meta=json.loads((S/'continue-owning-capi-source.json').read_text());state['source-before']=identity(source);assert(state['source-before']['bytes'],state['source-before']['sha256'])==(meta['bytes'],meta['sha256'])
 inputs=[ROOT/'include/assist/emulation/rax/rax_process.hpp',ROOT/'include/assist/emulation/rax/librax_loader.hpp',ROOT/'src/emulation/rax/rax_process.cpp',ROOT/'vendor/rax/capi/include/rax.h'];state['adapter-inputs-before']=[identity(x)for x in inputs]
 programs=[R/'tests/fixtures/user/windows/bin/arm64/smoke.exe',Path('C:/Windows/System32/whoami.exe')];state['programs-before']=[identity(x)for x in programs];state['runtime-before']=ordinary['runtime-after'];assert[identity(Path(x['path']))for x in state['runtime-before']]==state['runtime-before']
 def controls():
  rows=[]
  for p in programs:
   x=subprocess.run([str(p)],capture_output=True,timeout=30);rows.append({'exit-code':x.returncode,'stdout-hex':x.stdout.hex(),'stderr-hex':x.stderr.hex()})
  return rows
 state['native-before']=controls();assert all(x['exit-code']==0 for x in state['native-before']);save()
 env={k.upper():v for k,v in os.environ.items()};env.update({k.upper():v for k,v in json.loads((S/'mutant-native-compiler-env-epoch4.json').read_text(encoding='utf-8-sig')).items()});compiler=next(Path(x)/'cl.exe'for x in env['PATH'].split(';')if(Path(x)/'cl.exe').is_file());objects=ROOT/'target/native-attributes/continue-capi-objects';objects.mkdir(exist_ok=True);executable=ROOT/'target/native-attributes/cpp/continue-owning-capi.exe';args=[str(compiler),'/Fo'+str(objects)+os.sep,'/nologo','/std:c++20','/O1','/MT','/EHsc','/utf-8','/DNOMINMAX','/I'+str(ROOT/'include'),'/I'+str(ROOT/'vendor/rax/capi/include'),'/I'+str(ROOT/'json'),str(source),str(inputs[2]),str(archive),'ws2_32.lib','userenv.lib','ntdll.lib','bcrypt.lib','advapi32.lib','/Fe:'+str(executable)];run('build',args,env);state['executable-before']=identity(executable)
 log=run('ordinary',[str(executable),*[str(p)for p in programs]],env);state['outcomes']=[json.loads(x[len('api-result '):])for x in log.read_text().splitlines()if x.startswith('api-result ')];assert len(state['outcomes'])==2
 first,second=state['outcomes'];assert first['reason']==4 and first['exit-code']==0 and first['stdout-hex']==first['stderr-hex']=='';assert second['reason']==5 and'NtQueryVirtualMemory class 3'in json.dumps(second['diagnostic'])
 state['native-after']=controls();state['programs-after']=[identity(x)for x in programs];state['runtime-after']=[identity(Path(x['path']))for x in state['runtime-before']];state['source-after']=identity(source);state['carrier-after']=identity(archive);state['adapter-inputs-after']=[identity(x)for x in inputs];state['executable-after']=identity(executable)
 for name in['native','programs','runtime','source','carrier','adapter-inputs','executable']:assert state[name+'-before']==state[name+'-after'],name
 state.update({'state':'finished','exit-code':0,'scope':'Retained Assist C++ RaxProcess adapter, C ABI and actual owning carrier execute native-runtime smoke to EXITED4/code0. Whoami remains FAILED5/class3. Source/runtime/program/carrier/exe identities and native controls stable; slice4096 changes observer-turn granularity.'})
except BaseException:state.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:state['utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(state),flush=True);raise SystemExit(state['exit-code'])
