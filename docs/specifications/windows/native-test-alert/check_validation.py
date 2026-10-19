"""Portable proof checker: hashes, native corpus, compiled gates and owning receipts."""
from pathlib import Path
import argparse,hashlib,json,os,re,runpy

D=Path(__file__).resolve().parent
V=D/'validation'
ROOT=D.parents[3]
PREFIX='docs/specifications/windows/native-test-alert/'
CONFIG='9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c'
FAILURES={
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
 'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
}

def load(path):
 raw=path.read_bytes()
 return json.loads(raw.decode('utf-16')if raw[:2]in [b'\xff\xfe',b'\xfe\xff']else raw.decode('utf-8-sig'))

def identity(path):
 digest=hashlib.sha256();length=0
 with path.open('rb')as stream:
  for chunk in iter(lambda:stream.read(1048576),b''):
   digest.update(chunk);length+=len(chunk)
 return length,digest.hexdigest()

def bound(row,path):
 assert identity(path)==(row['bytes'],row['sha256']),str(path)

def output(row):
 path=V/row['log'];bound(row,path)
 return path.read_text(errors='replace')

def results(text):return re.findall(r'test result: ([^\r\n]+)',text)

def completed(name):
 value=load(V/name)
 assert value['state']=='finished'and value['exit-code']==0,name
 return value

def steps(value):return {row.get('name',row.get('gate')):row for row in value['steps']}

def check(host,source=ROOT):
 overlay_path=V/'integration-epoch4/overlay.json';overlay=load(overlay_path)if overlay_path.exists()else None
 changes={x['path']:x for x in overlay['files']}if overlay else{}
 if overlay:
  assert overlay['upstream-commit']=='1b25f3438f63d8cc256ae05ca4270e675a084163'and overlay['feature-code-unchanged']and len(changes)==111
  protected=load(V/'alert-ownership-epoch3.json')['source-owned-paths']
  assert len(protected)==9 and overlay['protected-feature-paths']==protected
  assert not(set(changes)&set(overlay['protected-feature-paths']))
 def source_bound(row):
  path=source/row['path'];raw=os.readlink(path).encode()if path.is_symlink()else path.read_bytes();actual={'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()};expected={k:row[k]for k in ['bytes','sha256']}
  if actual==expected:return
  if host=='windows'and row['path']=='.cargo/config.toml'and actual=={'bytes':374,'sha256':CONFIG}:return
  assert overlay and row['path']in changes and changes[row['path']]['before']==expected and changes[row['path']]['after']==actual,row['path']
 manifest=load(D/'evidence-manifest.json');names=set()
 for row in manifest['files']:
  assert row['path']not in names;names.add(row['path']);bound(row,D/row['path'])
 actual={str(x.relative_to(D)).replace(os.sep,'/')for x in D.rglob('*')if x.is_file()}
 assert actual==names|{'evidence-manifest.json'},'evidence file set differs'
 native=runpy.run_path(str(D/'check_native.py'))['check']()
 assert native['native-checks']==216 and native['native-callbacks']==144
 negative=load(V/'alert-native-negative-controls-v3.json')
 assert negative['all-rejected']and len(negative['controls'])==14 and all(x['rejected']for x in negative['controls'])
 final_negative=load(V/'alert-final-negative-controls.json')
 assert final_negative['all-rejected']and len(final_negative['controls'])>=16 and all(x['rejected']for x in final_negative['controls'])
 register=load(V/'final-assumptions.json')['assumptions'];assert [x['id']for x in register]==['A1','A2','A3','A4']
 for row in register:
  assert all(row.get(key)for key in ['assumption','basis','dependent-result','stress-test','falsification-probe'])
  assert row['status']in ['confirmed','revised']
 quality=load(V/'quality-gates.json')['gates'];assert set(quality)=={'QG'+str(i)for i in range(1,10)}and all(x.startswith('pass')for x in quality.values())
 frozen=load(V/'source-freeze.json')
 assert frozen['source-epoch']==3 and frozen['base-repository-commit']=='b8ef4239e577d07793ca08dcac83f25c36d7c81c'
 assert len(frozen['files'])==25088 and frozen['total-bytes']==1830323541
 for row in frozen['files']:
  if row['path'].startswith(PREFIX):continue
  source_bound(row)
 owning=load(V/'owning-source-hashes.json');assert owning['epoch']==3 and len(owning['sources'])==884
 for row in owning['sources']:source_bound(row)
 for row in owning['helpers']:bound(row,V/row['path'])
 base=load(V/'alert-baseline-a.json');assert base['exit-code']==101
 text=output(base);assert 'Finished `test`'in text and any('0 passed; 2 failed;'in x for x in results(text))and'not implemented'in text
 base=load(V/'alert-decoder-baseline-epoch3.json');assert base['exit-code']==101
 text=output(base);assert 'Finished `test`'in text and any('0 passed; 1 failed;'in x for x in results(text))and'complete installed zero-argument leaf must be admitted'in text
 focused=load(V/'alert-prototype-epoch3.json');assert focused['exit-code']==0
 assert any('8 passed; 0 failed;'in x for x in results(output(focused)))
 gate_counts={}
 for platform,count,focus in [('macos',8057,8),('linux',8051,8),('windows',7521,9)]:
  gates=completed('gates-'+platform+'.json');selected=steps(gates)
  expected={'full','capi','integration','all-targets'}|({'toolchain','focus'}if platform=='windows'else{'focus'}if platform=='linux'else set())
  assert set(selected)==expected,(platform,set(selected))
  for name,row in selected.items():
   text=output(row);assert row['results']==results(text)
   if name=='full':
    wanted=f'{count} passed; {5 if platform=="windows"else 0} failed; 2 ignored; 0 measured; 0 filtered out;'
    assert any(wanted in x for x in row['results']),(platform,row['results'])
    if platform=='windows':
     assert row['exit-code']==101
     last=text[text.rfind('\nfailures:\n'):]
     assert set(re.findall(r'^    (\S+)\r?$',last,re.M))==FAILURES
     assert set(gates['exact-existing-failures'])==FAILURES
    else:assert row['exit-code']==0
   else:assert row['exit-code']==0,(platform,name)
   if name=='focus':assert any(f'{focus} passed; 0 failed;'in x for x in row['results'])
   if name=='capi':assert any('183 passed; 0 failed;'in x for x in row['results'])
   if name=='integration':assert any(f'{4 if platform=="windows"else 544} passed; 0 failed;'in x for x in row['results'])
  gate_counts[platform]={'passed':count,'failed':5 if platform=='windows'else 0,'ignored':2}
 for platform in ['macos','linux','windows']:
  receipt=completed('owning-'+platform+'.json');rows=steps(receipt)
  assert len(rows)==(8 if platform=='macos'else 5 if platform=='linux'else 6)
  for row in rows.values():assert row['exit-code']==0;output(row)
  before=load(V/('archives-'+platform+'-before.json'));after=load(V/('archives-'+platform+'-after.json'))
  assert before['archives']==after['archives']and before['core-features']==after['core-features']==[]
  assert before['sources']==after['sources']==owning['sources']
  assert len(before['archives'])==2
  for archive in before['archives']:
   assert archive['structural-member-walk']=='pass'and archive['members']>0 and archive['bytes']>0
  if platform=='macos':
   ctest=output(rows['ctest'])
   assert re.search(r'100% tests passed(?:, 0 tests failed)? out of 7',ctest)
   assert len(re.findall(r'^\d+/7 Test .*\sPassed\s',ctest,re.M))==7
   linkage=load(V/'macos-alert-epoch3-linkage.json');assert len(linkage)==6
   assert {x['target']for x in linkage}=={'assist_rax_process_tests','assist_process_tool_tests','assist_rust_archive_link_tests','assist_e14_rax_abi_drift_tests','assist_mcp','assist_idalib_cli'}
  elif platform=='linux':
   text=output(rows['cpp']);assert text.count('linked-archive-before ')==text.count('linked-archive-after ')==1
 ordinary=completed('ordinary.json')
 for name in ['core','observer','executable','runtime']:assert ordinary[name+'-before']==ordinary[name+'-after'],name
 assert len(ordinary['runtime-before'])==6
 final_archives=load(V/'archives-windows-after.json')['archives'];core=next(x for x in final_archives if x['path'].endswith('.rlib'))
 assert(ordinary['core-before']['bytes'],ordinary['core-before']['sha256'])==(core['bytes'],core['sha256'])
 assert [x['name']for x in ordinary['outcomes']]==['smoke','whoami']
 all_steps=steps(ordinary)
 for row in all_steps.values():assert row['exit-code']==0;output(row)
 smoke=None;normalized=[]
 for row in ordinary['outcomes']:
  assert row['program-before']==row['program-after']and row['native-before']==row['native-after']
  assert row['native-before']['exit-code']==0
  path=V/row['log'];assert identity(path)[1]==row['log-sha256'];text=path.read_text()
  assert 'observation-finished 'in text and'terminal=true'in text
  end=re.search(r'terminal turn=(\d+) ([^\r\n]+)',text);assert end
  normalized.append({'name':row['name'],'terminal-turn':int(end[1]),'terminal':end[2],'native':row['native-before'],'alert-calls':re.findall(r'alert-call ([^\r\n]+)',text),'alert-results':re.findall(r'alert-result ([^\r\n]+)',text),'console':re.findall(r'guest-console ([^\r\n]+)',text)})
  assert ('NtContinue alertable APC delivery'if row['name']=='smoke'else'NtQueryVirtualMemory class 3')in end[2]
  if row['name']=='smoke':smoke=text
 assert smoke is not None
 calls=re.findall(r'alert-result turn=(\d+) tid=\d+ status=(0x[0-9a-f]+)',smoke)
 assert calls and int(calls[0][0])>=2943177 and int(calls[0][1],16)==0,'old frontier not exercised successfully'
 terminal=re.search(r'terminal turn=(\d+) ([^\r\n]+)',smoke);assert terminal and int(terminal[1])>2943177
 summary=load(V/'summary.json');assert summary['library-gates']==gate_counts and summary['ordinary-programs']['actual-core']==ordinary['core-before']
 assert summary['ordinary-programs']['outcomes']==normalized
 assert summary['native-top-level-checks']==216 and summary['native-callbacks']==144 and summary['modeled-top-level-checks']==152 and summary['modeled-callbacks']==80
 assert summary['full-process-goal-complete']is False
 assert summary['retained-assist-abi']==184 and summary['current-root-abi']==264 and summary['whole-current-root-products-validated']is False
 result={'host':host,'state':'verified','source-files':len(frozen['files']),'owning-source-inputs':884,'library-gates':gate_counts,'native-checks':216,'native-callbacks':144,'scope':'One Windows kernel; exact compiled selections and retained ABI184 consumers. Whole current-root products and full-process completion remain separate.'}
 if overlay:
  negative=load(V/'integration-epoch4/negative-controls.json')
  assert negative['all-rejected']and len(negative['controls'])==10 and all(x['rejected']for x in negative['controls'])
  result['pre-integration-library-gates']=gate_counts
  result['integration']=runpy.run_path(str(D/'check_integration.py'))['check'](host,source)
  result['library-gates']=result['integration']['library-gates']
 return result

if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('--host',required=True,choices=['macos','linux','windows']);parser.add_argument('--source',type=Path,default=ROOT);args=parser.parse_args();print(json.dumps(check(args.host,args.source),indent=2))
