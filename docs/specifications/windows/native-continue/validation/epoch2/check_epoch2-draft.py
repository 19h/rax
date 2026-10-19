"""Validate fresh exact-PC/SP APC ownership correction and actual owning execution."""
from pathlib import Path
import hashlib,json,os,re
D=Path(__file__).resolve().parent;V=D/'validation/epoch2';CONFIG='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
FAILURES={
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
 'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
}
def load(path):return json.loads(path.read_text(encoding='utf-8-sig'))
def bound(row,path):
 raw=path.read_bytes();assert(len(raw),hashlib.sha256(raw).hexdigest())==(row['bytes'],row['sha256']),str(path)
def text(row):
 path=V/row['log'];bound(row,path);return path.read_text(errors='replace')
def receipt(name):
 value=load(V/name);assert value['state']=='finished'and value['exit-code']==0,name;return value
def steps(value):return{x.get('name',x.get('gate')):x for x in value['steps']}
def check(host,source):
 frozen=load(V/'source-freeze.json');assert frozen['source-epoch']==2 and frozen['base-repository-commit']=='5d475727a72106d24fbd4178df69a6f85efbf85f';assert len(frozen['files'])==25070 and frozen['total-bytes']==1834269504
 for row in frozen['files']:
  p=source/row['path'];raw=os.readlink(p).encode()if p.is_symlink()else p.read_bytes();digest=hashlib.sha256(raw).hexdigest()
  assert(digest==row['sha256']and len(raw)==row['bytes'])or(host=='windows'and row['path']=='.cargo/config.toml'and digest==CONFIG and len(raw)==374),row['path']
 owning=load(V/'owning-source-hashes.json');assert owning['epoch']==2 and len(owning['sources'])==3812
 for row in owning['sources']:bound(row,source/row['path'])
 for row in owning['helpers']:bound(row,V/row['path'])
 summary=load(V/'summary.json');counts={}
 register=load(V/'assumptions.json')['assumptions'];assert len(register)==1 and register[0]['id']=='A5'and register[0]['status']=='confirmed'
 assert all(register[0].get(key)for key in ['assumption','basis','dependent-result','stress-test','falsification-probe'])
 quality=load(V/'quality-gates.json')['gates'];assert set(quality)=={'QG'+str(i)for i in range(1,10)}and all(x.startswith('pass')for x in quality.values())
 for platform,expected_count in [('macos',8082),('linux',8076),('windows',7543)]:
  value=receipt('gates-'+platform+'.json');selected=steps(value);expected={'full','capi','integration','all-targets'}|({'toolchain','focus'}if platform=='windows'else{'focus'}if platform=='linux'else set());assert set(selected)==expected
  for name,row in selected.items():
   out=text(row);results=re.findall(r'test result: ([^\r\n]+)',out);assert row['results']==results
   if name=='full':
    match=re.search(r'(\d+) passed; (\d+) failed; (\d+) ignored; 0 measured; 0 filtered out;',results[0]);assert match;passed,failed,ignored=map(int,match.groups());assert passed==expected_count and failed==(5 if platform=='windows'else 0)and ignored==2
    counts[platform]={'passed':passed,'failed':failed,'ignored':ignored}
    if platform=='windows':assert row['exit-code']==101 and set(value['exact-existing-failures'])==FAILURES and set(re.findall(r'^    (\S+)\r?$',out[out.rfind('\nfailures:\n'):],re.M))==FAILURES
    else:assert row['exit-code']==0
   else:assert row['exit-code']==0
   if name=='focus':assert any(f'{13 if platform=="windows"else 12} passed; 0 failed;'in x for x in results)
   if name=='capi':assert any('183 passed; 0 failed;'in x for x in results)
   if name=='integration':assert any(f'{4 if platform=="windows"else 544} passed; 0 failed;'in x for x in results)
 for platform in ['macos','linux','windows']:
  value=receipt('owning-'+platform+'.json');selected=steps(value);assert len(selected)==(8 if platform=='macos'else 5 if platform=='linux'else 6)
  for row in selected.values():assert row['exit-code']==0;text(row)
  before=load(V/('archives-'+platform+'-before.json'));after=load(V/('archives-'+platform+'-after.json'));assert before['archives']==after['archives']and before['sources']==after['sources']==owning['sources']and before['core-features']==after['core-features']==[]
  assert len(before['archives'])==2 and all(x['structural-member-walk']=='pass'and x['members']>0 for x in before['archives'])
  if platform=='macos':assert len(re.findall(r'^\d+/7 Test .*\sPassed\s',text(selected['ctest']),re.M))==7 and len(load(V/'macos-continue-epoch2-linkage.json'))==6
 ordinary=receipt('ordinary.json')
 for name in ['core','observer','executable','runtime']:assert ordinary[name+'-before']==ordinary[name+'-after']
 core=next(x for x in load(V/'archives-windows-after.json')['archives']if x['path'].endswith('.rlib'));assert(core['bytes'],core['sha256'])==(ordinary['core-before']['bytes'],ordinary['core-before']['sha256'])
 for row in ordinary['steps']:assert row['exit-code']==0;text(row)
 normalized=[]
 for row in ordinary['outcomes']:
  assert row['program-before']==row['program-after']and row['native-before']==row['native-after']and row['native-before']['exit-code']==0
  path=V/row['log'];assert hashlib.sha256(path.read_bytes()).hexdigest()==row['log-sha256'];out=path.read_text();end=re.search(r'terminal turn=(\d+) ([^\r\n]+)',out);assert end
  item={'name':row['name'],'terminal-turn':int(end[1]),'terminal':end[2],'native':row['native-before'],'continue-leaves':re.findall(r'continue-leaf ([^\r\n]+)',out),'continue-calls':re.findall(r'continue-call ([^\r\n]+)',out),'continue-results':re.findall(r'continue-result ([^\r\n]+)',out),'console':re.findall(r'guest-console ([^\r\n]+)',out)};normalized.append(item)
  if row['name']=='smoke':
   assert end[2]=='Complete(Exited(0))'and int(end[1])==3115529 and len(item['continue-calls'])==len(item['continue-results'])==1
   parse=lambda line:dict(x.split('=',1)for x in line.split()if'='in x)
   call,result=parse(item['continue-calls'][0]),parse(item['continue-results'][0]);assert call['test']=='1'and call['turn']==result['turn']=='2943204'
   for field in ['pc','sp','gpr0','gpr3']:assert int(call['target-'+field],16)==int(result[field],16)
   assert int(result['gpr0'],16)!=0 and result['frames']==result['queued']=='0'and result['wait-status']=='None'
  else:assert row['name']=='whoami'and'NtQueryVirtualMemory class 3'in end[2]and int(end[1])==2966197
 assert summary['library-gates']==counts and summary['ordinary-programs']=={'actual-core':ordinary['core-before'],'outcomes':normalized}and summary['full-process-goal-complete']is False
 import runpy
 native=runpy.run_path(str(D/'check_supplement.py'))['check']();assert(native['native-resumes'],native['native-post-checks'],native['native-callbacks'])==(16,32,28)
 baseline=load(V/'continue-exact-target-baseline.json');assert baseline['exit-code']==101 and baseline['inputs-unchanged']and'0 passed; 1 failed;'in text(baseline)
 assert summary['published-code-parent']=='5d475727a72106d24fbd4178df69a6f85efbf85f'and summary['whole-current-root-products-validated']is False
 capi=load(V/'capi.json');assert capi['state']=='finished'and capi['exit-code']==0
 for name in ['source','carrier','adapter-inputs','executable','programs','runtime','native']:assert capi[name+'-before']==capi[name+'-after']
 carrier=next(x for x in load(V/'archives-windows-after.json')['archives']if x['path'].endswith('.lib'));assert(carrier['bytes'],carrier['sha256'])==(capi['carrier-before']['bytes'],capi['carrier-before']['sha256'])
 for row in capi['steps']:assert row['exit-code']==0;text(row)
 raw=text(capi['steps'][1]);records=[json.loads(x[len('api-result '):])for x in raw.splitlines()if x.startswith('api-result ')];assert records==capi['outcomes']and len(records)==2
 assert records[0]['reason']==4 and records[0]['exit-code']==0 and records[0]['stdout-hex']==records[0]['stderr-hex']==''
 assert records[1]['reason']==5 and'NtQueryVirtualMemory class 3'in json.dumps(records[1]['diagnostic'])
 return {'source-epoch':2,'source-files':25070,'source-bytes':1834269504,'owning-source-inputs':3812,'library-gates':counts,'actual-core':ordinary['core-before'],'ordinary-smoke-exit':0,'full-process-goal-complete':False}
