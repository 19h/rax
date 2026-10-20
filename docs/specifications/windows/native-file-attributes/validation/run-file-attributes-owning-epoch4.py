from pathlib import Path
import datetime,hashlib,json,os,subprocess,sys,time,traceback
P=Path(__file__).parent;R=Path('/Users/int/.assist-file-attributes-20261020');HOST=sys.argv[1];assert HOST in ['macos','linux'];RECORD=P/(HOST+'-file-attributes-epoch4-owning-gates.json');assert not RECORD.exists()
manifest=json.loads((P/'file-attributes-owning-source-hashes-epoch3.json').read_text());commands=json.loads((P/(HOST+'-file-attributes-epoch4-owning-commands.json')).read_text());state={'state':'running','host':HOST,'pid':os.getpid(),'source-inputs':len(manifest['sources']),'scope':'Retained Assist ABI184 linked consumers; no whole current root GUI/plugin/package claim.','steps':[]}
def save():RECORD.write_text(json.dumps(state,indent=2)+'\n')
def verify():
 for row in manifest['sources']:
  raw=(R/row['path']).read_bytes();assert len(raw)==row['bytes']and hashlib.sha256(raw).hexdigest()==row['sha256'],row['path']
save()
try:
 for row in commands:
  verify();log=P/(HOST+'-file-attributes-epoch4-owning-'+row['gate']+'.log');started=time.monotonic()
  env=dict(os.environ,RUSTUP_TOOLCHAIN='1.95.0')
  if HOST=='macos':env['TMPDIR']=str(P.parent/'wait-completion-packet-create-20261012/assist-root-current/build-native-attributes/tmp')
  with log.open('wb')as out:
   child=subprocess.Popen(row['command'],cwd=row.get('cwd'),env=env,stdout=out,stderr=subprocess.STDOUT);state.update({'current-step':row['gate'],'child-pid':child.pid});save();code=child.wait()
  verify();raw=log.read_bytes();state['steps'].append({**row,'exit-code':code,'seconds':time.monotonic()-started,'log':log.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()});save();print(HOST,row['gate'],code,flush=True);assert code==0,row['gate']
 if HOST=='macos':
  before=P/'macos-file-attributes-epoch3-c-pre-cpp-archive-identities.json';after=P/'macos-file-attributes-epoch3-c-final-owning-archive-identities.json'
 else:before=P/'linux-file-attributes-epoch3-pre-cpp-archive-identities.json';after=P/'linux-file-attributes-epoch3-final-owning-archive-identities.json'
 a=json.loads(before.read_text());b=json.loads(after.read_text());assert a['archives']==b['archives']
 if HOST=='macos':
  build=P.parent/'wait-completion-packet-create-20261012/assist-root-current/build-native-attributes';selected=next(x for x in b['archives']if x['path'].endswith('.a'));links=[]
  for target in ['assist_rax_process_tests','assist_process_tool_tests','assist_rust_archive_link_tests','assist_e14_rax_abi_drift_tests','assist_mcp','assist_idalib_cli']:
   output=subprocess.check_output(['ninja','-C',str(build),'-t','commands',target],text=True);assert selected['path']in output,target;links.append({'target':target,'carrier':selected['path'],'commands-sha256':hashlib.sha256(output.encode()).hexdigest()})
  (P/'macos-file-attributes-epoch3-linkage.json').write_text(json.dumps(links,indent=2)+'\n')
 state.update({'state':'finished','exit-code':0,'archive-identities-stable':True})
except BaseException:state.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:state['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
raise SystemExit(state['exit-code'])
