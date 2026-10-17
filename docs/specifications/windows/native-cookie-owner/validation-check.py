"""Validate raw results, source/feature bindings and preserved failed attempts."""
from pathlib import Path
import hashlib,json,re,subprocess,sys
ROOT=Path(__file__).resolve().parent
V=ROOT/'validation'
RAX=ROOT.parents[3]
FAILURES={
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
 'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime'}
def need(x,m):
 if not x:raise ValueError(m)
def text(n):return (V/n).read_text(encoding='utf-8-sig')
def load(n):return json.loads(text(n))
def summaries(n):return [tuple(map(int,x)) for x in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;',text(n))]
def main():
 need(__debug__,'optimized verifier rejected')
 subprocess.run([sys.executable,'-B',str(ROOT/'check.py')],check=True)
 result=load('results.json');need(result['parent']=='8b038b528d8cd88e44e5ebd672ef14e365c480a5' and result['selected-owning-inputs']==310,'final epoch')
 seen=set()
 for x in result['files']:
  n=x['path'];need(n not in seen and not Path(n).is_absolute() and '..' not in Path(n).parts,'unique relative path');seen.add(n)
  d=(V/n).read_bytes();need((len(d),hashlib.sha256(d).hexdigest())==(x['bytes'],x['sha256']),n)
 for host,count,oldcount,oldfail in [('macos',7839,7832,1),('linux',7833,7825,2)]:
  for phase in ['initial','final']:
   gs=load(f'{host}/{phase}/gates.json');need(len(gs)==4,'core selection')
   for g in gs:
    need(g['exit-code']==(101 if phase=='initial' and g['gate']=='lib' else 0),'core gate status')
    need(hashlib.sha256((V/host/phase/g['log']).read_bytes()).hexdigest()==g['sha256'],'gate log identity')
  need(summaries(f'{host}/final/{host}-cookie-final-lib.log')==[(count,0,2,0)],'unfiltered final library '+host)
  need(summaries(f'{host}/initial/{host}-cookie-lib.log')==[(oldcount,oldfail,2,0)],'retained initial failure '+host)
  need((168,0,0,0) in summaries(f'{host}/final/{host}-cookie-final-capi.log'),'C API '+host)
  need((544,0,0,0) in summaries(f'{host}/final/{host}-cookie-final-windows.log'),'integration '+host)
  gs=load(host+'-owning-gates.json');need(len(gs)==(6 if host=='macos' else 4) and all(g['exit-code']==0 for g in gs),'owning gates '+host)
  for g in gs:need(hashlib.sha256((V/g['log']).read_bytes()).hexdigest()==g['sha256'],'owning log identity')
 native='windows/native-cookie-owner-final-'
 gs=load(native+'verified-gates.json')['gates'];need(len(gs)==6,'six native core gates')
 for g in gs:
  need(g['exit-code']==(101 if g['gate']=='full' else 0),'native gate status')
  need(hashlib.sha256((V/'windows'/g['log']).read_bytes()).hexdigest()==g['sha256'],'native raw log binding')
 need(summaries(native+'full.log')==[(7263,5,2,0)],'native full library')
 raw=text(native+'full.log');need(set(re.findall(r'^    (\S+)$',raw[raw.rfind('\nfailures:\n'):],re.M))==FAILURES,'exact retained lowerer failures')
 need(any(x[:3]==(3,0,0) for x in summaries(native+'focused.log')),'three new focused tests')
 need(any(x[:3]==(7,0,0) for x in summaries(native+'prior-services.log')),'seven prior owner tests')
 need((168,0,0,0) in summaries(native+'capi.log'),'native C API')
 need((4,0,0,0) in summaries(native+'integration.log'),'native memory')
 initial=load('windows/cookie-owner-final-launch.json')
 need(initial['core-exit-code']==0 and initial['owning-exit-code']==101 and initial['exit-code']==1,'preserve initial owning failure')
 need('failed to load manifest' in text('windows-initial-owning-failure/assist-native-cookie-owner-final-shipping.log'),'missing junction evidence')
 need('no targets specified' in text('windows-first-recovery-failure/assist-native-cookie-owner-final-recovered-shipping.log'),'incomplete cache evidence')
 restored=load('windows/cookie-owner-final-owning-restored.json')
 need(restored['state']=='finished' and restored['exit-code']==0 and restored['verified-files']==10001,'completed owning-only restoration')
 need(restored['configs']['C:\\Windows\\Temp\\assist-native-rax-20261010\\src\\.cargo\\config.toml']=='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c','original core adapter unchanged')
 manifest=load('final-owning-source-hashes.json')
 need(len(manifest['sources'])==310,'selected source count')
 for x in manifest['sources']:
  d=(RAX/x['path']).read_bytes();need((len(d),hashlib.sha256(d).hexdigest())==(x['bytes'],x['sha256']),'current source '+x['path'])
 for name in ['macos-final-owning-archive-identities.json','linux-final-owning-archive-identities.json',native+'restored-owning-archive-hashes.json']:
  a=load(name);need(a['sources']==manifest['sources'] and a['core-features']==[],'same sources and empty features '+name)
  need(len(a['archives'])==2 and all(x['members']>200 and x['structural-member-walk']=='pass' for x in a['archives']),'whole archive walk '+name)
 need('100% tests passed out of 7' in text('macos-owning-ctest.log'),'seven CTests')
 for name in ['linux-owning-cpp.log','windows/assist-native-cookie-owner-final-restored-cpp.log']:
  for marker in ['rust_archive_link','rax_process','process_tool','process_tool_unavailable','rax_abi_drift']:need(marker in text(name),'C++ consumer '+marker)
 boundary=load('owning-validation-boundary.json');need((boundary['frozen-assist-public-abi-minor'],boundary['current-assist-public-abi-minor'],boundary['rax-public-abi'])==(184,186,'1.11.0'),'explicit frozen carrier boundary')
 for row in load(native+'ordinary-outcomes.json'):
  if 'log' in row:
   raw=text('windows/'+row['log']);need('NtQueryInformationToken' in raw and ('TURN' in raw or 'turn' in raw),'actual next boundary')
   need(hashlib.sha256((V/'windows'/row['log']).read_bytes()).hexdigest()==row['sha256'],'ordinary log binding')
 print('Verified three-host full libraries, core and scoped embedding gates, 310 actual source inputs, complete archives and retained failures')
if __name__=='__main__':main()
