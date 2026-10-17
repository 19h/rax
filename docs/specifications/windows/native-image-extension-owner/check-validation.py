"""Verify raw final gates, exact source epochs, owned archives and ordinary limits."""
from pathlib import Path
import argparse,hashlib,json,re
D=Path(__file__).resolve().parent
V=D/'validation'
parser=argparse.ArgumentParser();parser.add_argument('--source-root',type=Path,default=D.parents[3]);args=parser.parse_args()
P=args.source_root
def need(ok,message):
 if not ok:raise ValueError(message)
def load(path):
 raw=path.read_bytes();return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe',b'\xfe\xff'] else raw.decode('utf-8-sig'))
def counts(name):
 text=(V/name).read_text(encoding='utf-8-sig')
 return [tuple(map(int,x))for x in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out;',text)]
need(__debug__,'optimized validation is rejected')
for name in ['macos-image-extension-final-full.log','linux-image-extension-final-full.log','native-image-extension-final-full.log']:
 raw=(V/name).read_text(encoding='utf-8-sig');ignored=set(re.findall(r'^test (\S+) \.\.\. ignored$',raw,re.M))
 need(ignored=={'smir::lower::validation::tests::test_lift_lower_full_microkernel','smir::lower::validation::tests::test_roundtrip_exact_bytes_microkernel'},'ignored test identities')
manifest=load(V/'inputs.json');seen=set()
for row in manifest['files']:
 need(row['path'] not in seen,'duplicate validation path');seen.add(row['path'])
 raw=(D/row['path']).read_bytes()
 need(len(raw)==row['bytes'] and hashlib.sha256(raw).hexdigest()==row['sha256'],'validation input '+row['path'])
epoch5=load(V/'epoch5-frozen-source.json')['files']
epoch6=load(V/'epoch6-frozen-source.json')['files']
changed='src/user/windows/process/sched/tests/services_tests/image_extension_owner_leaf_tests.rs'
change=load(V/'epoch6-change.json')
need(hashlib.sha256((V/'epoch6-owner-leaf-before.rs').read_bytes()).hexdigest()==epoch5[changed],'archived epoch5 leaf')
need('#[cfg(windows)]' in (P/change['test-only-parent']).read_text(),'Windows-only test ownership')
need({name for name in epoch5 if epoch5[name]!=epoch6[name]}=={changed},'only Windows test changed in epoch6')
for name,digest in epoch6.items():
 need(hashlib.sha256((P/name).read_bytes()).hexdigest()==digest,'final source '+name)
for host,total in [('macos',7878),('linux',7872)]:
 rows=load(V/(host+'-image-extension-final-gates.json'))['gates']
 need([r['gate']for r in rows]==['focus','full','capi','integration','all-targets'],'Unix gate set')
 for row in rows:
  need(row['exit-code']==0,'Unix failed gate')
  need(hashlib.sha256((V/row['log']).read_bytes()).hexdigest()==row['sha256'],'Unix gate log digest')
 need(counts(host+'-image-extension-final-focus.log')[0][0:3]==(18,0,0),'Unix focus counts')
 need(counts(host+'-image-extension-final-full.log')==[(total,0,2,0)],'Unix full counts')
 need((168,0,0,0) in counts(host+'-image-extension-final-capi.log'),'Unix C API')
 need(sum(r[0]for r in counts(host+'-image-extension-final-integration.log'))==544,'Unix integration count')
 for row in load(V/(host+'-image-owning-gates.json')):
  need(row['exit-code']==0,'Unix owning gate')
  need(hashlib.sha256((V/row['log']).read_bytes()).hexdigest()==row['sha256'],'Unix owning log digest')
 archive=load(V/(host+'-image-final-owning-archive-identities.json'))
 need(len(archive['archives'])==2 and archive['core-features']==[],'Unix owned core features')
 need(len(archive['sources'])==342,'Unix selected owning source count')
 for row in archive['sources']:
  raw=(P/row['path']).read_bytes();need((len(raw)==row['bytes'] and hashlib.sha256(raw).hexdigest()==row['sha256'] or row['path']==changed and row['sha256']==epoch5[changed] and row['bytes']==change['before-bytes']),'Unix owning source '+row['path'])
 for row in archive['archives']:
  need(row['structural-member-walk']=='pass' and row['members']>0 and row['bytes']>0,'Unix complete archive walk')
for name in ['image-extension-final-v2-launch.json','image-extension-final-owned-launch.json']:
 row=load(V/name);need(row['state']=='finished' and row['exit-code']==0,'native final driver '+name)
need(counts('native-image-extension-final-focus-focus.log')==[(20,0,0,7296)],'native focus')
need(counts('native-image-extension-epoch6-focus.log')==[(20,0,0,7296)],'epoch6 native focus')
need(all(row['exit-code']==0 for row in load(V/'native-image-extension-epoch6-gates.json')),'epoch6 gates')
rows=load(V/'native-image-extension-final-gates.json')
need([r['gate']for r in rows]==['integration','capi','full','all-targets'],'native gate set')
for row in rows:need(row['exit-code']==(101 if row['gate']=='full'else 0),'native gate '+row['gate'])
need(counts('native-image-extension-final-full.log')==[(7309,5,2,0)],'native full counts')
text=(V/'native-image-extension-final-full.log').read_text(encoding='utf-8-sig')
failed=set(re.findall(r'^    (\S+)$',text[text.rfind('\nfailures:\n'):],re.M))
known={
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
 'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime',
}
need(failed==known,'native known failure identities')
need((168,0,0,0) in counts('native-image-extension-final-capi.log'),'native C API')
need((4,0,0,0) in counts('native-image-extension-final-integration.log'),'native memory adapter')
archive=load(V/'native-image-extension-final-owning-archive-hashes.json')
need(archive['core-features']==[] and len(archive['archives'])==2,'native owned archives')
need(counts('native-image-extension-final-integration.log')==[(0,0,0,0),(4,0,0,0)],'native integration selection')
for row in archive['archives']:need(row['structural-member-walk']=='pass' and row['members']>0,'native complete member walk')
need(len(archive['sources'])==342,'native owning source identities')
for row in archive['sources']:
 raw=(P/row['path']).read_bytes();need((len(raw)==row['bytes'] and hashlib.sha256(raw).hexdigest()==row['sha256'] or row['path']==changed and row['sha256']==epoch5[changed] and row['bytes']==change['before-bytes']),'native owning source '+row['path'])
cpp=(V/'assist-native-image-extension-final-cpp.log').read_text()
for token in ['182 checks passed','162 checks passed','2/2 passed','PASS','canonical discovery and explicit RAX-disabled refusal passed']:
 need(token in cpp,'native C++ consumer '+token)
ordinary=load(V/'ordinary-final-analysis.json')
executed=[r for r in ordinary if 'terminal'in r]
need(len(executed)==2,'ordinary program count')
for row in executed:
 need(row['guest_exit'] is None and row['controller-exit-code']==0,'ordinary exit accounting')
 need('NtQuerySystemTime (0x5a)'in row['terminal'][1],'ordinary boundary')
 need(hashlib.sha256((V/row['log']).read_bytes()).hexdigest()==row['sha256'],'ordinary raw log identity')
need([int(r['terminal'][0])for r in executed]==[97964,98028],'ordinary observer identity')
print('final image-extension validation: exact source, all gates, baseline failures, archives and incomplete ordinary runs verified')
