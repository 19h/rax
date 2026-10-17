from pathlib import Path
import hashlib,json,re,subprocess,sys
D=Path(__file__).resolve().parent;V=D/'validation';SOURCE=D.parents[3]
KNOWN={
'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime'}
def need(v,m):
 if not v:raise ValueError(m)
def load(n):return json.loads((V/n).read_text(encoding='utf-8-sig'))
def text(n):return (V/n).read_text(encoding='utf-8-sig')
def sums(n):return [tuple(map(int,r))for r in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;',text(n))]
def gates(n,expected):
 rr=load(n);rr=rr if isinstance(rr,list)else[rr]
 need(len(rr)==len(expected)and {x['gate']for x in rr}==set(expected),'complete unique gates '+n)
 for r in rr:
  need(r['exit-code']==expected[r['gate']],'gate exit '+n)
  if 'sha256'in r:
   p=('windows/'if n.startswith('windows/')else'')+r['log'];need(hashlib.sha256((V/p).read_bytes()).hexdigest()==r['sha256'],'gate log '+p)
 return rr

def main():
 need(__debug__,'optimized verification rejected')
 subprocess.run([sys.executable,'-B',str(D/'check.py')],check=True)
 m=load('results.json');need(m['base']=='44d21496b9c81e3911e7bbb4160eb13b22ca183d'and m['selected-owning-inputs']==332,'source base/count')
 seen=set()
 for r in m['files']:
  n=r['path'];need(n not in seen and not Path(n).is_absolute()and '..'not in Path(n).parts,'unique relative proof');seen.add(n)
  raw=(V/n).read_bytes();need((len(raw),hashlib.sha256(raw).hexdigest())==(r['bytes'],r['sha256']),n)
 epochs={}
 for label,n in [('initial','initial-source-inputs.json'),('final','epoch2-source-inputs.json')]:
  rr=load(n)['files'];need(len(rr)==11,'eleven compiled inputs');epochs[label]={x['path']:x for x in rr}
  for r in rr:
   raw=(V/'compiled-source'/label/r['path']).read_bytes();need((len(raw),hashlib.sha256(raw).hexdigest())==(r['bytes'],r['sha256']),'compiled source '+label+'/'+r['path'])
 need(set(epochs['initial'])==set(epochs['final']),'unchanged compiled input paths')
 changed={n for n in epochs['initial']if epochs['initial'][n]['sha256']!=epochs['final'][n]['sha256']}
 need(changed=={'docs/specifications/windows/native-wnf-system-subscribe/cases.json','src/user/windows/wnf/system_tests.rs','src/user/windows/process/sched/tests/services_tests/wnf_system_subscription_tests.rs','src/user/windows/process/sched/tests/services_tests/wnf_system_subscription_leaf_tests.rs'},'only fixture correction and zero-length test extension')
 need(load('epoch2-correction.json')['production-changed']is False,'production unchanged across correction')
 for host,total in [('macos',7865),('linux',7859)]:
  gates(host+'-wnf-system-initial-gates.json',{'system':101,'regression':101,'all-targets':0})
  need(any(x[:3]==(6,1,0)for x in sums(host+'-wnf-system-initial-system.log')),'retained initial failure')
  need('state1'in text(host+'-wnf-system-initial-system.log'),'original observation mismatch')
  gates(host+'-wnf-system-epoch2-gates.json',dict.fromkeys(['system','regression','all-targets'],0))
  need(any(x[:3]==(7,0,0)for x in sums(host+'-wnf-system-epoch2-system.log')),'seven focused tests')
  need(any(x[:3]==(40,0,0)for x in sums(host+'-wnf-system-epoch2-regression.log')),'forty WNF regressions')
  gates(host+'-wnf-system-final-gates.json',dict.fromkeys(['all-targets','windows','capi','lib'],0))
  need(sums(host+'-wnf-system-final-lib.log')==[(total,0,2,0)],'full library '+host)
  need((544,0,0,0)in sums(host+'-wnf-system-final-windows.log'),'Windows integration '+host)
  need((168,0,0,0)in sums(host+'-wnf-system-final-capi.log'),'C API '+host)
  expected=dict.fromkeys(['build','ctest','protected-assist-cli','protected-assist-mcp','artifacts','archive-identities']if host=='macos'else['shipping','cpp','artifacts','archive-identities'],0)
  gates(host+'-system-owning-gates.json',expected)
 gates('windows/native-wnf-system-initial-gates.json',{'system':101,'regression':101,'all-targets':0})
 need(any(x[:3]==(6,2,0)for x in sums('windows/native-wnf-system-initial-system.log')),'retained native fixture/leaf failures')
 gates('windows/native-wnf-system-epoch2-gates.json',dict.fromkeys(['system','regression','all-targets'],0))
 need(any(x[:3]==(8,0,0)for x in sums('windows/native-wnf-system-epoch2-system.log')),'native focus includes installed leaves')
 need(any(x[:3]==(46,0,0)for x in sums('windows/native-wnf-system-epoch2-regression.log')),'native WNF regression count')
 rr=load('windows/native-wnf-system-final-verified-gates.json')['gates'];need(len(rr)==3 and {r['gate']for r in rr}=={'integration','capi','full'},'complete native final gates')
 for r in rr:
  need(r['exit-code']==(101 if r['gate']=='full'else 0),'native final exit')
  need(hashlib.sha256((V/'windows'/r['log']).read_bytes()).hexdigest()==r['sha256'],'native final log')
 need(sums('windows/native-wnf-system-final-full.log')==[(7295,5,2,0)],'native full count')
 raw=text('windows/native-wnf-system-final-full.log');need(set(re.findall(r'^    (\S+)$',raw[raw.rfind('\nfailures:\n'):],re.M))==KNOWN,'exact pre-existing Windows failures')
 need((168,0,0,0)in sums('windows/native-wnf-system-final-capi.log'),'native C API')
 need((4,0,0,0)in sums('windows/native-wnf-system-final-integration.log'),'native memory')
 chain=load('windows/wnf-system-final-launch.json');need(chain['state']=='finished'and chain['exit-code']==chain['epoch2-exit-code']==chain['core-exit-code']==chain['owning-exit-code']==0,'complete native chain')
 sources=load('final-owning-source-hashes.json')['sources'];need(len(sources)==332,'332 selected sources')
 for r in sources:
  raw=(SOURCE/r['path']).read_bytes();need((len(raw),hashlib.sha256(raw).hexdigest())==(r['bytes'],r['sha256']),'current selected source '+r['path'])
 for n in ['macos-system-final-owning-archive-identities.json','linux-system-final-owning-archive-identities.json','windows/native-wnf-system-final-owning-archive-hashes.json']:
  a=load(n);need(a['sources']==sources and a['core-features']==[],'archive inputs/features '+n)
  need(len(a['archives'])==2 and all(x['members']>200 and x['structural-member-walk']=='pass'for x in a['archives']),'complete archive walks '+n)
 need('100% tests passed out of 7'in text('macos-system-owning-ctest.log'),'seven CTests')
 for n in ['linux-system-owning-cpp.log','windows/assist-native-wnf-system-final-cpp.log']:
  for marker in ['rust_archive_link','rax_process','process_tool','process_tool_unavailable','rax_abi_drift']:need(marker in text(n),'C++ consumer '+marker)
 b=load('owning-validation-boundary.json');need((b['frozen-assist-public-abi-minor'],b['current-assist-public-abi-minor'],b['rax-public-abi'])==(184,186,'1.11.0'),'frozen embedding scope')
 ordinary=load('windows/native-wnf-system-final-ordinary-outcomes.json');need(len(ordinary)==3 and sum('log'in x for x in ordinary)==2,'ordinary observations and excluded absent fixture')
 expected={'325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b','48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6'};actual=set()
 for r in ordinary:
  if 'log'in r:
   actual.add(r['program-sha256']);need(hashlib.sha256((V/'windows'/r['log']).read_bytes()).hexdigest()==r['sha256'],'ordinary raw log')
 need(actual==expected,'ordinary executable identities')
 print('Verified System WNF source epochs, three-host full suites, native leaves and scoped332-input embedding')
if __name__=='__main__':main()
