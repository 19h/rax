"""Check source epochs, real selections and scoped three-host embedding."""
from pathlib import Path
import hashlib,json,re,subprocess,sys
R=Path(__file__).resolve().parent;V=R/'validation';SOURCE=R.parents[3]
KNOWN={
'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime'}
def need(x,m):
 if not x:raise ValueError(m)
def load(n):return json.loads((V/n).read_text(encoding='utf-8-sig'))
def text(n):return (V/n).read_text(encoding='utf-8-sig')
def summaries(n):return [tuple(map(int,r))for r in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;',text(n))]
def gates(n,expected):
 rows=load(n);rows=rows if isinstance(rows,list)else[rows]
 need(len(rows)==len(expected)and {x['gate']for x in rows}==set(expected),'complete unique gates '+n)
 for x in rows:
  need(x['exit-code']==expected[x['gate']],'gate exit '+n+'/'+x['gate'])
  if 'sha256'in x:
   p=('windows/'if n.startswith('windows/')else'')+x['log']
   need(hashlib.sha256((V/p).read_bytes()).hexdigest()==x['sha256'],'gate log identity '+p)
 return rows

def main():
 need(__debug__,'optimized proof verification rejected')
 subprocess.run([sys.executable,'-B',str(R/'check.py')],check=True)
 m=load('results.json');need(m['parent']=='7279261c3ab518e0d0b027817cabf776e9700eb8'and m['selected-owning-inputs']==327,'source parent and input count')
 seen=set()
 for row in m['files']:
  n=row['path'];need(n not in seen and not Path(n).is_absolute()and '..'not in Path(n).parts,'unique relative record');seen.add(n)
  raw=(V/n).read_bytes();need((len(raw),hashlib.sha256(raw).hexdigest())==(row['bytes'],row['sha256']),n)
 epochs={}
 for label,name,count in [('initial','initial-source-inputs.json',11),('epoch2','epoch2-source-inputs.json',11),('epoch3','epoch3-source-inputs.json',12),('final','epoch4-source-inputs.json',13)]:
  rows=load(name)['files'];need(len(rows)==count,'source epoch cardinality '+label);epochs[label]={x['path']:x for x in rows}
  for row in rows:
   raw=(V/'compiled-source'/label/row['path']).read_bytes();need((len(raw),hashlib.sha256(raw).hexdigest())==(row['bytes'],row['sha256']),'compiled '+label+'/'+row['path'])
 need(set(epochs['initial'])==set(epochs['epoch2']),'unchanged paths for correction')
 changed={n for n in epochs['initial']if epochs['initial'][n]['sha256']!=epochs['epoch2'][n]['sha256']}
 need(changed=={'src/user/windows/process/sched/tests/services_tests.rs','src/user/windows/wnf/query_tests.rs','src/user/windows/wnf/installed_query.rs'},'exact compile/host-thread correction')
 changed={n for n in epochs['epoch2']if epochs['epoch2'][n]['sha256']!=epochs['epoch3'][n]['sha256']}
 need(changed=={'src/user/windows/process/sched/tests/services_tests/wnf_query_tests.rs'},'only scope replay extends prior test source')
 need(set(epochs['epoch3'])-set(epochs['epoch2'])=={'docs/specifications/windows/native-wnf-query/controls/wnf-private-scope-order.json'},'one extra compiled native scope fixture')
 changed={n for n in epochs['epoch3']if epochs['epoch3'][n]['sha256']!=epochs['final'][n]['sha256']}
 need(changed=={'src/user/windows/dll/native/wnf_query.rs','src/user/windows/process/sched/tests/services_tests/wnf_query_tests.rs'},'exact uncaptured namespace correction')
 need(set(epochs['final'])-set(epochs['epoch3'])=={'docs/specifications/windows/native-wnf-query/controls/wnf-owned-cross-scope.json'},'one native falsification fixture')
 need(load('epoch4-scope-falsification.json')['final-selected-inputs']==327,'recorded native falsification')
 for host,full in [('macos',7858),('linux',7852)]:
  initial=load(host+'-wnf-initial-gates.json');need(initial[0]['gate']=='query'and initial[0]['exit-code']==101,'initial compile failure '+host)
  raw=text(host+'-wnf-initial-query.log');need('error[E0583]'in raw and 'error[E0433]'in raw,'retain actual initial module/import errors')
  for epoch,service in [('epoch2',5),('epoch3',6),('epoch4',6)]:
   gates(host+'-wnf-'+epoch+'-gates.json',dict.fromkeys(['query','service','subscription','access'],0))
   for suffix,count in [('query',4),('service',service),('subscription',4),('access',6)]:
    need(any(x[:3]==(count,0,0)for x in summaries(host+'-wnf-'+epoch+'-'+suffix+'.log')),'focused '+host+'/'+epoch+'/'+suffix)
  gates(host+'-wnf-final4-gates.json',dict.fromkeys(['all-targets','windows','capi','lib'],0))
  need(summaries(host+'-wnf-final4-lib.log')==[(full,0,2,0)],'complete unfiltered library '+host)
  need((544,0,0,0)in summaries(host+'-wnf-final4-windows.log'),'shared Windows integration '+host)
  need((168,0,0,0)in summaries(host+'-wnf-final4-capi.log'),'C API '+host)
  expected=dict.fromkeys(['build','ctest','protected-assist-cli','protected-assist-mcp','artifacts','archive-identities']if host=='macos'else['shipping','cpp','artifacts','archive-identities'],0)
  gates(host+'-owning4-gates.json',expected)
 initial=load('windows/native-wnf-query-initial-gates.json');initial=initial if isinstance(initial,list)else[initial]
 need(initial[0]['gate']=='query'and initial[0]['exit-code']==101,'native initial compile failure')
 need('error[E0583]'in text('windows/native-wnf-query-initial-query.log'),'actual native missing test module')
 need(load('windows/wnf-query-initial-stop.json')['exit-code']==0,'task-owned redundant initial retries stopped')
 gates('windows/native-wnf-query-epoch2-gates.json',dict.fromkeys(['query','service','subscription','access','all-targets'],0))
 for suffix,count in [('query',6),('service',6),('subscription',4),('access',7)]:
  need(any(x[:3]==(count,0,0)for x in summaries('windows/native-wnf-query-epoch2-'+suffix+'.log')),'native corrected '+suffix)
 need(load('windows/wnf-query-epoch3-launch.json')['exit-code']==1 and 'System.Char'in text('windows/wnf-query-epoch3-driver.log'),'retain original singleton PowerShell runner failure')
 gates('windows/native-wnf-query-epoch3-retry-gates.json',{'service':0})
 need(any(x[:3]==(7,0,0)for x in summaries('windows/native-wnf-query-epoch3-retry-service.log')),'native final scope and installed leaves')
 gates('windows/native-wnf-query-epoch4-gates.json',dict.fromkeys(['query','service','subscription','access','all-targets'],0))
 for suffix,count in [('query',6),('service',7),('subscription',4),('access',7)]:
  need(any(x[:3]==(count,0,0)for x in summaries('windows/native-wnf-query-epoch4-'+suffix+'.log')),'native corrected namespace '+suffix)
 need(load('windows/wnf-epoch3-final-stopped.json')['exit-code']==0,'stopped superseded native source')
 final=load('windows/native-wnf-query-final4-verified-gates.json')['gates'];need({x['gate']for x in final}=={'integration','capi','full'}and len(final)==3,'complete native final gates')
 for row in final:
  need(row['exit-code']==(101 if row['gate']=='full'else 0),'native final gate status')
  need(hashlib.sha256((V/'windows'/row['log']).read_bytes()).hexdigest()==row['sha256'],'native final log')
 need(summaries('windows/native-wnf-query-final4-full.log')==[(7287,5,2,0)],'native full count')
 raw=text('windows/native-wnf-query-final4-full.log');need(set(re.findall(r'^    (\S+)$',raw[raw.rfind('\nfailures:\n'):],re.M))==KNOWN,'exact retained native failures')
 need((168,0,0,0)in summaries('windows/native-wnf-query-final4-capi.log'),'native C API')
 need((4,0,0,0)in summaries('windows/native-wnf-query-final4-integration.log'),'native memory tests')
 r=load('windows/wnf-query-final4-launch.json');need(r['state']=='finished'and r['exit-code']==r['epoch4-exit-code']==r['core-exit-code']==r['owning-exit-code']==0,'native chain complete')
 sources=load('final-owning-source-hashes.json')['sources'];need(len(sources)==327,'327 source inputs')
 for row in sources:
  raw=(SOURCE/row['path']).read_bytes();need((len(raw),hashlib.sha256(raw).hexdigest())==(row['bytes'],row['sha256']),'current source '+row['path'])
 for name in ['macos-final4-owning-archive-identities.json','linux-final4-owning-archive-identities.json','windows/native-wnf-query-final4-owning-archive-hashes.json']:
  a=load(name);need(a['sources']==sources and a['core-features']==[],'archive source/features '+name)
  need(len(a['archives'])==2 and all(x['members']>200 and x['structural-member-walk']=='pass'for x in a['archives']),'complete archive walks '+name)
 need('100% tests passed out of 7'in text('macos-owning4-ctest.log'),'seven CTests')
 for name in ['linux-owning4-cpp.log','windows/assist-native-wnf-query-final4-cpp.log']:
  for marker in ['rust_archive_link','rax_process','process_tool','process_tool_unavailable','rax_abi_drift']:need(marker in text(name),'C++ consumer '+marker)
 bound=load('owning-validation-boundary.json');need((bound['frozen-assist-public-abi-minor'],bound['current-assist-public-abi-minor'],bound['rax-public-abi'])==(184,186,'1.11.0'),'frozen carrier boundary')
 ordinary=load('windows/native-wnf-query-final4-ordinary-outcomes.json');need(len(ordinary)==3 and sum('log'in x for x in ordinary)==2,'two real observations and one absent fixture')
 expected_ordinary={
  '325730e33e981784bff4a60bc6f342cf95c2735e85042c89748fc471cbe7781b':2141884,
  '48fde190b002a5a4c4b8b19db014377263ba94220f23a4f0a117fec082d46bf6':2144246}
 observed=set()
 for row in ordinary:
  if 'log'not in row:
   need('absent'in row.get('unavailable',''),'absent fixture not an execution')
   continue
  need(hashlib.sha256((V/'windows'/row['log']).read_bytes()).hexdigest()==row['sha256'],'ordinary raw log')
  identity=row['program-sha256'];need(identity in expected_ordinary and identity not in observed,'actual ordinary executable identity');observed.add(identity)
  raw=text('windows/'+row['log']);terminal=[line for line in raw.splitlines()if line.startswith('terminal ')]
  need(len(terminal)==1 and terminal[0].startswith('terminal turn='+str(expected_ordinary[identity])+' Complete(Internal(')and 'NtSubscribeWnfStateChange'in terminal[0],'ordinary is unsupported subscription, not a successful guest exit')
  need(row['controller-exit-code']==0 and 'PC=0x180002d10'in raw,'controller completion and actual next service PC')
 need(observed==set(expected_ordinary),'both ordinary programs observed')
 print('Verified WNF query epochs, exact native failures, complete three-host suites and scoped327-input embedding')
if __name__=='__main__':main()
