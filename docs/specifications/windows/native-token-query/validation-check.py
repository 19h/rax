"""Verify final token code, real core counts and scoped three-host embedding."""
from pathlib import Path
import hashlib,json,re,subprocess,sys
ROOT=Path(__file__).resolve().parent;V=ROOT/'validation';RAX=ROOT.parents[3]
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
 result=load('results.json');need(result['parent']=='aa9bcc5651164ade3265e124de9a6d4f778f561f' and result['selected-owning-inputs']==313,'final source epoch')
 seen=set()
 for r in result['files']:
  n=r['path'];need(n not in seen and not Path(n).is_absolute() and '..' not in Path(n).parts,'unique relative evidence');seen.add(n)
  d=(V/n).read_bytes();need((len(d),hashlib.sha256(d).hexdigest())==(r['bytes'],r['sha256']),n)
 a=load('epoch1-code-inputs.json')['files'];b=load('epoch2-code-inputs.json')['files']
 need(len(a)==len(b)==11 and [x['path'] for x in a]==[x['path'] for x in b],'eleven inputs in both epochs')
 changed=[(x,y) for x,y in zip(a,b) if x['sha256']!=y['sha256']]
 need(len(changed)==1 and changed[0][0]['path']=='src/user/windows/process/sched/tests/services_tests/token_query_tests.rs','only Windows test reference changes')
 for epoch,rows in [(1,a),(2,b)]:
  for r in rows:
   d=(V/f'compiled-source/epoch{epoch}'/r['path']).read_bytes();need((len(d),hashlib.sha256(d).hexdigest())==(r['bytes'],r['sha256']),'compiled source epoch')
 test_path=changed[0][0]['path'];old=(V/'compiled-source/epoch1'/test_path).read_text();new=(V/'compiled-source/epoch2'/test_path).read_text()
 marker='#[cfg(windows)]\n#[test]\nfn native_token_query_installed_leaves'
 need(marker in old and old.split(marker)[0]==new.split(marker)[0],'Unix compiled items identical')
 need(old.replace('super::super::virtual_memory_tests::installed::invoke(', 'super::virtual_memory_tests::installed::invoke(')==new,'exact excluded module reference change')
 initial=load('windows/token-query-final-launch.json');need(initial['exit-code']==1 and initial['core-exit-code']==101,'retain original Windows compile failure')
 need('error[E0433]' in text('windows/native-token-query-final-focused.log'),'retain original inaccessible helper error')
 for h,count in [('macos',7842),('linux',7836)]:
  gates=load(h+'-token-final-gates.json');need(len(gates)==7 and all(g['exit-code']==0 for g in gates),'complete core '+h)
  for g in gates:need(hashlib.sha256((V/g['log']).read_bytes()).hexdigest()==g['sha256'],'core log binding')
  need(summaries(h+'-token-final-lib.log')==[(count,0,2,0)],'unfiltered full '+h)
  for suffix,n in [('focused',2),('subject',1),('cookie',3)]:need(any(x[:3]==(n,0,0) for x in summaries(h+'-token-final-'+suffix+'.log')),'actual '+suffix+' '+h)
  need((168,0,0,0) in summaries(h+'-token-final-capi.log'),'C API '+h)
  need((544,0,0,0) in summaries(h+'-token-final-windows.log'),'integration '+h)
  final=load(h+'-token-epoch2-gates.json');need(len(final)==4 and all(g['exit-code']==0 for g in final),'four final Unix gates')
  for g in final:need(hashlib.sha256((V/g['log']).read_bytes()).hexdigest()==g['sha256'],'final Unix log identity')
  for suffix,n in [('focused',2),('subject',1),('cookie',3)]:need(any(x[:3]==(n,0,0) for x in summaries(h+'-token-epoch2-'+suffix+'.log')),'actual final '+suffix)
  gates=load(h+'-owning-gates.json');need(len(gates)==(6 if h=='macos' else 4) and all(g['exit-code']==0 for g in gates),'owning '+h)
  for g in gates:need(hashlib.sha256((V/g['log']).read_bytes()).hexdigest()==g['sha256'],'owning log binding')
 prefix='windows/native-token-query-epoch2-'
 gates=load(prefix+'verified-gates.json')['gates'];need(len(gates)==7,'seven native gates')
 for g in gates:
  need(g['exit-code']==(101 if g['gate']=='full' else 0),'native gate status')
  need(hashlib.sha256((V/'windows'/g['log']).read_bytes()).hexdigest()==g['sha256'],'native raw log binding')
 need(summaries(prefix+'full.log')==[(7267,5,2,0)],'native full count')
 raw=text(prefix+'full.log');need(set(re.findall(r'^    (\S+)$',raw[raw.rfind('\nfailures:\n'):],re.M))==FAILURES,'exact retained native lowerer failures')
 for suffix,n in [('focused',3),('subject',1),('cookie',3)]:need(any(x[:3]==(n,0,0) for x in summaries(prefix+suffix+'.log')),'actual native '+suffix)
 need((168,0,0,0) in summaries(prefix+'capi.log'),'native C API')
 need((4,0,0,0) in summaries(prefix+'integration.log'),'native memory')
 launch=load('windows/token-query-epoch2-launch.json');need(launch['state']=='finished' and launch['exit-code']==launch['core-exit-code']==launch['owning-exit-code']==0,'native driver completion')
 m=load('epoch2-owning-source-hashes.json');need(len(m['sources'])==313,'owning source count')
 for x in m['sources']:
  d=(RAX/x['path']).read_bytes();need((len(d),hashlib.sha256(d).hexdigest())==(x['bytes'],x['sha256']),'current source '+x['path'])
 for name in ['macos-final-owning-archive-identities.json','linux-final-owning-archive-identities.json',prefix+'owning-archive-hashes.json']:
  a=load(name);need(a['sources']==m['sources'] and a['core-features']==[],'source and features '+name)
  need(len(a['archives'])==2 and all(x['members']>200 and x['structural-member-walk']=='pass' for x in a['archives']),'complete archives '+name)
 need('100% tests passed out of 7' in text('macos-owning-ctest.log'),'seven CTests')
 for name in ['linux-owning-cpp.log','windows/assist-native-token-query-epoch2-cpp.log']:
  for marker in ['rust_archive_link','rax_process','process_tool','process_tool_unavailable','rax_abi_drift']:need(marker in text(name),'C++ consumer '+marker)
 b=load('owning-validation-boundary.json');need((b['frozen-assist-public-abi-minor'],b['current-assist-public-abi-minor'],b['rax-public-abi'])==(184,186,'1.11.0'),'frozen carrier boundary')
 print('Verified final token source, three-host core/owning counts, installed leaves, 313 inputs and complete archives')
if __name__=='__main__':main()
