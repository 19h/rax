"""Portable source, native-reference, selected gate and owning-execution audit."""
from pathlib import Path
import argparse,hashlib,json,os,re,runpy
D=Path(__file__).resolve().parent;V=D/'validation';ROOT=D.parents[3]
CONFIG='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
FAILURES={
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
 'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
}
def load(path):return json.loads(path.read_text(encoding='utf-8-sig'))
def identity(path):
 digest=hashlib.sha256();size=0
 with path.open('rb')as stream:
  for raw in iter(lambda:stream.read(1048576),b''):digest.update(raw);size+=len(raw)
 return size,digest.hexdigest()
def bound(row,path):assert identity(path)==(row['bytes'],row['sha256']),str(path)
def text(row):
 path=V/row['log'];bound(row,path);return path.read_text(errors='replace')
def receipt(name):
 d=load(V/name);assert d['state']=='finished'and d['exit-code']==0,name;return d
def selected(value):return{x.get('name',x.get('gate')):x for x in value['steps']}
def fields(line):return dict(x.split('=',1)for x in line.split()if'='in x)
def check(host,source=ROOT):
 overlay_path=V/'epoch3/overlay.json';overlay=load(overlay_path)if overlay_path.exists()else None
 changes={x['path']:x for x in overlay['files']}if overlay else{}
 def source_bound(row):
  p=source/row['path'];raw=os.readlink(p).encode()if p.is_symlink()else p.read_bytes();actual={'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()};expected={k:row[k]for k in['bytes','sha256']}
  if actual==expected:return
  if host=='windows'and row['path']=='.cargo/config.toml'and actual=={'bytes':374,'sha256':CONFIG}:return
  assert overlay and row['path']in changes and changes[row['path']]['before']==expected and changes[row['path']]['after']==actual,row['path']
 envelope=load(D/'evidence-manifest.json');names=set()
 for row in envelope['files']:
  assert row['path']not in names;names.add(row['path']);bound(row,D/row['path'])
 assert{p.relative_to(D).as_posix()for p in D.rglob('*')if p.is_file()}==names|{'evidence-manifest.json'}
 native=runpy.run_path(str(D/'check_native.py'))['check']();assert(native['native-resumes'],native['native-post-checks'],native['native-callbacks'])==(52,104,48)
 kernel=receipt('continue-kernel-profile.json');assert(kernel['major'],kernel['minor'],kernel['build'],kernel['native-processor-architecture'],kernel['RtlGetVersion-status'])==(10,0,29683,12,0)
 negative=load(V/'continue-native-negative-controls.json');assert negative['all-rejected']and len(negative['controls'])==13 and all(x['rejected']for x in negative['controls'])
 assumptions=load(V/'final-assumptions.json')['assumptions'];assert[x['id']for x in assumptions]==['A1','A2','A3','A4']
 for row in assumptions:
  assert row['status']in['confirmed','revised']and all(row.get(k)for k in['assumption','basis','dependent-result','stress-test','falsification-probe'])
 quality=load(V/'quality-gates.json')['gates'];assert set(quality)=={'QG'+str(i)for i in range(1,10)}and all(x.startswith('pass')for x in quality.values())
 frozen=load(V/'source-freeze.json');assert frozen['source-epoch']==1 and len(frozen['files'])==25069 and frozen['total-bytes']==1834242339
 for row in frozen['files']:source_bound(row)
 transition=load(V/'continue-parent-publication-integration.json');assert transition['previous-parent']==frozen['base-repository-commit']=='711876bcf74e4113fe9ce2d31b439033243fc5c0'and transition['published-parent']=='5d475727a72106d24fbd4178df69a6f85efbf85f'and transition['compiled-input-intersection']==[]and transition['verified-frozen-source-inputs']==25069 and transition['ownership-preserved']is True
 assert not set(transition['proof-only-parent-diff'])&{x['path']for x in frozen['files']}
 owning=load(V/'owning-source-hashes.json');assert owning['epoch']==1 and len(owning['sources'])==3811
 for row in owning['sources']:source_bound(row)
 for row in owning['helpers']:bound(row,V/row['path'])
 base=load(V/'continue-baseline-a.json');out=text(base);assert base['exit-code']==101 and'Finished `test`'in out and'0 passed; 3 failed;'in out and'NtContinue alertable APC delivery'in out
 frame=load(V/'continue-focus-c.json');out=text(frame);assert frame['exit-code']==101 and'7 passed; 1 failed;'in out and'replaced APC continuation must not survive'in out
 prototype=load(V/'continue-focus-d.json');assert prototype['exit-code']==0 and'9 passed; 0 failed;'in text(prototype)
 summary=load(V/'summary.json');counts={}
 for platform,passed in[('macos',8079),('linux',8073),('windows',7540)]:
  gates=receipt('gates-'+platform+'.json');steps=selected(gates);assert set(steps)==({'full','capi','integration','all-targets'}|({'toolchain','focus'}if platform=='windows'else{'focus'}if platform=='linux'else set()))
  for name,row in steps.items():
   out=text(row);results=re.findall(r'test result: ([^\r\n]+)',out);assert row['results']==results
   if name=='full':
    failed=5 if platform=='windows'else 0;assert f'{passed} passed; {failed} failed; 2 ignored; 0 measured; 0 filtered out;'in results[0];counts[platform]={'passed':passed,'failed':failed,'ignored':2}
    if platform=='windows':assert row['exit-code']==101 and set(gates['exact-existing-failures'])==FAILURES and set(re.findall(r'^    (\S+)\r?$',out[out.rfind('\nfailures:\n'):],re.M))==FAILURES
    else:assert row['exit-code']==0
   else:assert row['exit-code']==0
   if name=='focus':assert any(f'{10 if platform=="windows"else 9} passed; 0 failed;'in x for x in results)
   if name=='capi':assert any('183 passed; 0 failed;'in x for x in results)
   if name=='integration':assert any(f'{4 if platform=="windows"else 544} passed; 0 failed;'in x for x in results)
  owner=receipt('owning-'+platform+'.json');steps=selected(owner);assert len(steps)==(8 if platform=='macos'else 5 if platform=='linux'else 6)
  for row in steps.values():assert row['exit-code']==0;text(row)
  before=load(V/('archives-'+platform+'-before.json'));after=load(V/('archives-'+platform+'-after.json'));assert before['archives']==after['archives']and before['sources']==after['sources']==owning['sources']and before['core-features']==after['core-features']==[]
  assert len(before['archives'])==2 and all(x['structural-member-walk']=='pass'and x['members']>0 for x in before['archives'])
  if platform=='macos':assert len(re.findall(r'^\d+/7 Test .*\sPassed\s',text(steps['ctest']),re.M))==7 and len(load(V/'macos-continue-epoch1-linkage.json'))==6
 ordinary=receipt('ordinary.json');assert set(selected(ordinary))=={'build','smoke','whoami'}
 for name in['core','observer','executable','runtime']:assert ordinary[name+'-before']==ordinary[name+'-after']
 for row in ordinary['steps']:assert row['exit-code']==0;text(row)
 core=next(x for x in load(V/'archives-windows-after.json')['archives']if x['path'].endswith('.rlib'));assert(core['bytes'],core['sha256'])==(ordinary['core-before']['bytes'],ordinary['core-before']['sha256'])
 observer=load(V/'continue-observer-source-epoch1.json');bound(observer,V/'continue-ordinary-owner-epoch1.rs');assert(observer['bytes'],observer['sha256'])==(ordinary['observer-before']['bytes'],ordinary['observer-before']['sha256'])
 outcomes=[]
 for row in ordinary['outcomes']:
  assert row['program-before']==row['program-after']and row['native-before']==row['native-after']and row['native-before']['exit-code']==0
  path=V/row['log'];assert identity(path)[1]==row['log-sha256'];out=path.read_text();end=re.search(r'terminal turn=(\d+) ([^\r\n]+)',out);assert end and'observation-finished 'in out and'terminal=true'in out
  item={'name':row['name'],'terminal-turn':int(end[1]),'terminal':end[2],'native':row['native-before'],'continue-leaves':re.findall(r'continue-leaf ([^\r\n]+)',out),'continue-calls':re.findall(r'continue-call ([^\r\n]+)',out),'continue-results':re.findall(r'continue-result ([^\r\n]+)',out),'console':re.findall(r'guest-console ([^\r\n]+)',out)};outcomes.append(item)
  if row['name']=='smoke':
   assert end[2]=='Complete(Exited(0))'and int(end[1])==3115529 and len(item['continue-calls'])==len(item['continue-results'])==1
   call,result=fields(item['continue-calls'][0]),fields(item['continue-results'][0]);assert call['test']=='1'and call['turn']==result['turn']=='2943204'
   for field in['pc','sp','gpr0','gpr3']:assert int(call['target-'+field],16)==int(result[field],16)
   assert int(result['gpr0'],16)!=0 and result['frames']==result['queued']=='0'and result['wait-status']=='None'
   assert len(re.findall(r'alert-result turn=2943177 tid=\d+ status=0x0 ',out))==1
   for console in item['console']:assert fields(console)['bytes']=='0'and fields(console)['hex']==''
  else:assert row['name']=='whoami'and'NtQueryVirtualMemory class 3'in end[2]and int(end[1])==2966197 and not item['continue-calls']
 retained=receipt('continue-native-retained-artifacts.json');assert len(retained['artifacts'])==3
 expected=[*load(V/'archives-windows-after.json')['archives'],ordinary['executable-after']]
 for row in retained['artifacts']:
  assert(row['source']['bytes'],row['source']['sha256'])==(row['retained']['bytes'],row['retained']['sha256'])and any((x['bytes'],x['sha256'])==(row['source']['bytes'],row['source']['sha256'])for x in expected)
 assert summary['library-gates']==counts and summary['ordinary-programs']=={'actual-core':ordinary['core-before'],'outcomes':outcomes}and summary['ordinary-smoke-success']is True and summary['full-process-goal-complete']is False and summary['whole-current-root-products-validated']is False
 capi=runpy.run_path(str(D/'check_capi.py'))['check']();assert summary['owning-cpp-capi']==capi
 negative=load(V/'continue-final-negative-controls.json');assert len(negative['controls'])==summary['final-corruptions-rejected']==20 and negative['all-rejected']and all(x['rejected']for x in negative['controls'])
 for platform in ['macos','linux','windows']:
  proof=receipt('continue-final-check-'+platform+'-round1.json');assert len(proof['steps'])==1
  row=proof['steps'][0];assert row['name']=='portable-proof'and row['exit-code']==0
  checked=json.loads(text(row));assert checked['host']==platform and checked['state']=='verified'and checked['library-gates']==counts
  assert summary['portable-proof-round1'][platform]=={'exit-code':0,'envelope':proof['envelope']}
 result={'host':host,'state':'verified','source-files':25069,'source-bytes':1834242339,'owning-source-inputs':3811,'library-gates':counts,'native-resumes':52,'native-post-checks':104,'native-callbacks':48,'ordinary-smoke-exit':0,'whoami-frontier':'NtQueryVirtualMemory class3','full-process-goal-complete':False,'scope':'Exact selections and retained ABI184 consumers; current-root whole GUI/plugin/packages remain separate.'}
 if overlay:
  assert len(changes)==16
  current=runpy.run_path(str(D/'check_epoch3.py'))['check'](host,source);result['epoch1-library-gates']=counts;result['epoch3']=current
  for name in ['source-files','source-bytes','owning-source-inputs','library-gates']:result[name]=current[name]
  result.update({'native-resumes':68,'native-post-checks':136,'native-callbacks':76})
 return result
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--host',required=True,choices=['macos','linux','windows']);p.add_argument('--source',type=Path,default=ROOT);args=p.parse_args();print(json.dumps(check(args.host,args.source),indent=2))
