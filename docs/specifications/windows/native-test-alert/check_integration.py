"""Validate the source and actual owning artifacts after concurrent-master integration."""
from pathlib import Path
import hashlib,json,os,re
D=Path(__file__).resolve().parent;V=D/'validation/integration-epoch4';CONFIG='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
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
 frozen=load(V/'source-freeze.json');assert frozen['source-epoch']==4 and frozen['base-repository-commit']=='1b25f3438f63d8cc256ae05ca4270e675a084163';assert len(frozen['files'])==25066 and frozen['total-bytes']==1834179374
 for row in frozen['files']:
  p=source/row['path'];raw=os.readlink(p).encode()if p.is_symlink()else p.read_bytes();digest=hashlib.sha256(raw).hexdigest()
  assert(digest==row['sha256']and len(raw)==row['bytes'])or(host=='windows'and row['path']=='.cargo/config.toml'and digest==CONFIG and len(raw)==374),row['path']
 owning=load(V/'owning-source-hashes.json');assert owning['epoch']==4 and len(owning['sources'])==3808
 for row in owning['sources']:bound(row,source/row['path'])
 for row in owning['helpers']:bound(row,V/row['path'])
 summary=load(V/'summary.json');counts={}
 register=load(V/'assumptions.json')['assumptions'];assert len(register)==1 and register[0]['id']=='A5'and register[0]['status']=='confirmed'
 assert all(register[0].get(key)for key in ['assumption','basis','dependent-result','stress-test','falsification-probe'])
 quality=load(V/'quality-gates.json')['gates'];assert set(quality)=={'QG'+str(i)for i in range(1,10)}and all(x.startswith('pass')for x in quality.values())
 for platform,expected_count in [('macos',8070),('linux',8064),('windows',7530)]:
  value=receipt('gates-'+platform+'.json');selected=steps(value);expected={'full','capi','integration','all-targets'}|({'toolchain','focus'}if platform=='windows'else{'focus'}if platform=='linux'else set());assert set(selected)==expected
  for name,row in selected.items():
   out=text(row);results=re.findall(r'test result: ([^\r\n]+)',out);assert row['results']==results
   if name=='full':
    match=re.search(r'(\d+) passed; (\d+) failed; (\d+) ignored; 0 measured; 0 filtered out;',results[0]);assert match;passed,failed,ignored=map(int,match.groups());assert passed==expected_count and failed==(5 if platform=='windows'else 0)and ignored==2
    counts[platform]={'passed':passed,'failed':failed,'ignored':ignored}
    if platform=='windows':assert row['exit-code']==101 and set(value['exact-existing-failures'])==FAILURES and set(re.findall(r'^    (\S+)\r?$',out[out.rfind('\nfailures:\n'):],re.M))==FAILURES
    else:assert row['exit-code']==0
   else:assert row['exit-code']==0
   if name=='focus':assert any(f'{9 if platform=="windows"else 8} passed; 0 failed;'in x for x in results)
   if name=='capi':assert any('183 passed; 0 failed;'in x for x in results)
   if name=='integration':assert any(f'{4 if platform=="windows"else 544} passed; 0 failed;'in x for x in results)
 for platform in ['macos','linux','windows']:
  value=receipt('owning-'+platform+'.json');selected=steps(value);assert len(selected)==(8 if platform=='macos'else 5 if platform=='linux'else 6)
  for row in selected.values():assert row['exit-code']==0;text(row)
  before=load(V/('archives-'+platform+'-before.json'));after=load(V/('archives-'+platform+'-after.json'));assert before['archives']==after['archives']and before['sources']==after['sources']==owning['sources']and before['core-features']==after['core-features']==[]
  assert len(before['archives'])==2 and all(x['structural-member-walk']=='pass'and x['members']>0 for x in before['archives'])
  if platform=='macos':assert len(re.findall(r'^\d+/7 Test .*\sPassed\s',text(selected['ctest']),re.M))==7 and len(load(V/'macos-alert-epoch4-linkage.json'))==6
 ordinary=receipt('ordinary.json')
 for name in ['core','observer','executable','runtime']:assert ordinary[name+'-before']==ordinary[name+'-after']
 core=next(x for x in load(V/'archives-windows-after.json')['archives']if x['path'].endswith('.rlib'));assert(core['bytes'],core['sha256'])==(ordinary['core-before']['bytes'],ordinary['core-before']['sha256'])
 for row in ordinary['steps']:assert row['exit-code']==0;text(row)
 normalized=[]
 for row in ordinary['outcomes']:
  assert row['program-before']==row['program-after']and row['native-before']==row['native-after']and row['native-before']['exit-code']==0
  path=V/row['log'];assert hashlib.sha256(path.read_bytes()).hexdigest()==row['log-sha256'];out=path.read_text();end=re.search(r'terminal turn=(\d+) ([^\r\n]+)',out);assert end
  normalized.append({'name':row['name'],'terminal-turn':int(end[1]),'terminal':end[2],'native':row['native-before'],'alert-calls':re.findall(r'alert-call ([^\r\n]+)',out),'alert-results':re.findall(r'alert-result ([^\r\n]+)',out),'console':re.findall(r'guest-console ([^\r\n]+)',out)})
  assert('NtContinue alertable APC delivery'if row['name']=='smoke'else'NtQueryVirtualMemory class 3')in end[2]
  if row['name']=='smoke':
   call=re.search(r'alert-result turn=(\d+) tid=\d+ status=0x0 ',out);assert call and int(end[1])>int(call[1])
 assert summary['library-gates']==counts and summary['ordinary-programs']['actual-core']==ordinary['core-before']and summary['ordinary-programs']['outcomes']==normalized and summary['full-process-goal-complete']is False
 assert summary['upstream-commit']=='1b25f3438f63d8cc256ae05ca4270e675a084163'and summary['feature-code-unchanged']is True and summary['whole-current-root-products-validated']is False
 for platform in ['macos','linux','windows']:
  proof=receipt('alert-final-check-epoch4-'+platform+'-round1.json');assert len(proof['steps'])==1
  row=proof['steps'][0];assert row['name']=='portable-proof'and row['exit-code']==0
  checked=load(V/row['log']);text(row);assert checked['host']==platform and checked['state']=='verified'and checked['integration']['library-gates']==counts
  assert summary['portable-proof-round1'][platform]=={'exit-code':0,'envelope':proof['envelope']}
 return{'source-epoch':4,'source-files':len(frozen['files']),'owning-source-inputs':3808,'library-gates':counts,'actual-core':ordinary['core-before'],'scope':'Fresh complete gates and actual owning continuation on the combined source; historical epoch3 receipts remain separately bounded.'}
