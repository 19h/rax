from pathlib import Path
import datetime,hashlib,json,os,re,subprocess,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
record=S/'image-extension-final-v2-launch.json';lock=B/'image-extension-final-v2-driver.lock'
def load(path):
 raw=path.read_bytes();return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe',b'\xfe\xff'] else raw.decode('utf-8-sig'))
def need(v,m):
 if not v:raise RuntimeError(m)
def summaries(path):return [list(map(int,row))for row in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;',path.read_text(encoding='utf-8-sig'))]
old=load(S/'image-extension-epoch1-launch.json');need(old.get('state')=='finished','epoch1 native driver is not complete')
if old.get('exit-code')!=0:
 gates=load(S/'native-image-extension-epoch1-gates.json');need({g['gate']:g['exit-code']for g in gates}=={'focus':0,'regression':101,'all-targets':0},'unexpected epoch1 gate state')
 raw=(S/'native-image-extension-epoch1-regression.log').read_text(encoding='utf-8-sig');failed=set(re.findall(r'^    (\S+)$',raw[raw.rfind('\nfailures:\n'):],re.M))
 need(failed=={'user::windows::process::sched::tests::services_tests::virtual_memory_tests::installed::installed_ntdll_virtual_memory_leaves_query_guest_images_and_nls_all_selected_abis'},'unexpected epoch1 failure')

need(not record.exists(),'native final driver already requested');fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.write(fd,str(os.getpid()).encode());os.close(fd)
r={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'starting','base':'bb697c7da3400d3fc2905cfbcfd7b4e68306ceee','source-epoch':5}
def save():record.write_text(json.dumps(r,indent=2)+'\n')
save()
try:
 for stage in ['focus','core']:
  argv=['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/('native-image-extension-final-'+stage+'.ps1'))]
  with(S/('image-extension-final-v2-'+stage+'-driver.log')).open('wb') as f:
   child=subprocess.Popen(argv,stdout=f,stderr=subprocess.STDOUT);r.update({'state':'running-'+stage,'child-pid':child.pid,'argv':argv});save();code=child.wait()
  r[stage+'-exit-code']=code;save();need(code==0,'native '+stage+' stage failed')
  if stage=='focus':
   gates=load(S/'native-image-extension-final-focus-gates.json');need(len(gates)==1 and gates[0]['exit-code']==0,'focused gates')
   counts=summaries(S/gates[0]['log']);need(len(counts)==1 and counts[0][0]>0 and counts[0][1:3]==[0,0],'focused test count');r['focus-counts']=counts;save()
  else:
   gates=load(S/'native-image-extension-final-gates.json');need(len(gates)==4,'final gate count')
   for g in gates:need(g['exit-code']==(101 if g['gate']=='full'else 0),'native '+g['gate']+' exit')
   counts=summaries(S/'native-image-extension-final-full.log');need(len(counts)==1 and counts[0][0]>0 and counts[0][1:]==[5,2,0],'full native count')
   raw=(S/'native-image-extension-final-full.log').read_text(encoding='utf-8-sig');failed=set(re.findall(r'^    (\S+)$',raw[raw.rfind('\nfailures:\n'):],re.M))
   known={'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry','smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry','smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry','smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards','smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime'}
   need(failed==known,'native failure identity drift');need([168,0,0,0]in summaries(S/'native-image-extension-final-capi.log'),'native C API count');need([4,0,0,0]in summaries(S/'native-image-extension-final-integration.log'),'native memory count')
   r['full-counts']=counts;save()
 r.update({'state':'finished','exit-code':0})
except BaseException:r.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:r['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();lock.unlink()
print(json.dumps(r),flush=True)
raise SystemExit(r['exit-code'])
