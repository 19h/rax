"""Serialize native source extension, ordinary observation, full and owning gates."""
from pathlib import Path
import datetime,hashlib,json,os,re,subprocess,time,traceback
B=Path('C:/Windows/Temp/assist-native-rax-20261010');S=Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
R=S/'wnf-query-final-retry-launch.json';LOCK=B/'wnf-query-final-retry-driver.lock'
KNOWN={
'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime'}
def need(v,m):
 if not v:raise RuntimeError(m)
def load(name):return json.loads((S/name).read_text(encoding='utf-8-sig'))
def summaries(name):return [list(map(int,r))for r in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;',(S/name).read_text(encoding='utf-8-sig'))]
need(__debug__,'optimized chain rejected');need(not R.exists(),'native chain already requested')
fd=os.open(LOCK,os.O_CREAT|os.O_EXCL|os.O_WRONLY);os.write(fd,str(os.getpid()).encode());os.close(fd)
r={'pid':os.getpid(),'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'waiting-epoch2','base':'7279261c3ab518e0d0b027817cabf776e9700eb8'}
def save():R.write_text(json.dumps(r,indent=2)+'\n')
save()
try:
 deadline=time.monotonic()+1800
 while True:
  dep=load('wnf-query-epoch2-launch.json')
  if dep.get('state') in ['finished','failed']:
   need(dep.get('exit-code')==0,'native epoch2 failed');break
  need(time.monotonic()<deadline,'native epoch2 did not finish');time.sleep(5)
 stages=[('epoch3',[str(B/'python/python.exe'),'-B',str(S/'run-wnf-query-epoch3-retry-atomic.py')]),
 ('core',['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/'native-wnf-query-final-gates.ps1')]),
 ('owning',['powershell.exe','-NoProfile','-ExecutionPolicy','Bypass','-File',str(S/'assist-native-wnf-query-final-owned.ps1')])]
 for label,args in stages:
  with (S/('wnf-query-final-retry-'+label+'-driver.log')).open('wb') as out:
   child=subprocess.Popen(args,stdout=out,stderr=subprocess.STDOUT);r.update({'state':'running-'+label,'child-pid':child.pid,'argv':args});save();code=child.wait()
  r[label+'-exit-code']=code;save();need(code==0,'native '+label+' stage failed')
  if label=='epoch3':
   gates=load('native-wnf-query-epoch3-retry-gates.json');gates=gates if isinstance(gates,list)else[gates]
   need(len(gates)==1 and gates[0]['gate']=='service' and gates[0]['exit-code']==0,'scope replay stage')
   need(any(x[:3]==[7,0,0]for x in summaries(gates[0]['log'])),'seven final native service tests')
  elif label=='core':
   gates=load('native-wnf-query-final-gates.json');need(len(gates)==3 and {g['gate']for g in gates}=={'integration','capi','full'},'three final core gates')
   for g in gates:
    need(g['exit-code']==(101 if g['gate']=='full'else 0),'native core exit status');g['sha256']=hashlib.sha256((S/g['log']).read_bytes()).hexdigest()
   need(summaries('native-wnf-query-final-full.log')==[[7287,5,2,0]],'actual unfiltered native count')
   raw=(S/'native-wnf-query-final-full.log').read_text(encoding='utf-8-sig')
   need(set(re.findall(r'^    (\S+)$',raw[raw.rfind('\nfailures:\n'):],re.M))==KNOWN,'exact previous Windows failures')
   need([168,0,0,0]in summaries('native-wnf-query-final-capi.log'),'native C API count')
   need([4,0,0,0]in summaries('native-wnf-query-final-integration.log'),'native memory count')
   (S/'native-wnf-query-final-verified-gates.json').write_text(json.dumps({'gates':gates,'full':[7287,5,2,0]},indent=2)+'\n')
  else:
   a=load('native-wnf-query-final-owning-archive-hashes.json')
   need(len(a['sources'])==326 and a['core-features']==[] and len(a['archives'])==2,'complete source-bound owning artifacts')
   need(all(x['members']>200 and x['structural-member-walk']=='pass'for x in a['archives']),'complete owning archive walks')
 r.update({'state':'finished','exit-code':0})
except BaseException:r.update({'state':'failed','exit-code':1,'error':traceback.format_exc()})
finally:r['completed-utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save();LOCK.unlink()
print(json.dumps(r),flush=True)
raise SystemExit(r['exit-code'])
