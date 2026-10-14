from pathlib import Path
import hashlib,json,re,sys
if not __debug__:raise RuntimeError('optimized final validation rejected')
base=Path(sys.argv[1]) if len(sys.argv)>1 else Path(__file__).parent
repo=base.parents[3]
def require(ok,why):
    if not ok:raise ValueError(why)
def text(name):
    data=(base/name).read_bytes()
    return data.decode('utf-16' if data.startswith(b'\xff\xfe') else 'utf-8-sig').replace('\r\n','\n')
def obj(name):return json.loads(text(name))
def results(name):return [list(map(int,r)) for r in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',text(name))]
def identity(path,item):
    raw=path.read_bytes();require(len(raw)==item['bytes'] and hashlib.sha256(raw).hexdigest()==item['sha256'],'source bytes '+str(path))
require(obj('format-check-exit.json')['exit-code']==0,'owned formatting')
full={h:results(h+'-latest-full.log') for h in ['macos','linux']}
full['windows']=results('native-section-map-latest-full.log')
require(full=={'macos':[[7763,0,2]],'linux':[[7757,0,2]],'windows':[[7178,5,2]]},'full counts')
focus={h:results(h+'-latest-focused.log') for h in ['macos','linux']};focus['windows']=results('native-section-map-final-projection-focused.log')
require(focus=={'macos':[[4,0,0]],'linux':[[4,0,0]],'windows':[[5,0,0]]},'focused selection')
capi={h:results(h+'-latest-capi.log') for h in ['macos','linux']};capi['windows']=results('native-section-map-latest-capi.log')
require(capi=={h:[[168,0,0],[0,0,0]] for h in ['macos','linux','windows']},'C API counts')
integration={'macos':results('macos-latest-integration.log'),'linux':results('linux-final-integration.log'),'windows':results('native-section-map-latest-integration.log')}
require(integration=={'macos':[[544,0,0],[0,0,0]],'linux':[[544,0,0],[0,0,0]],'windows':[[0,0,0],[4,0,0]]},'integration host selection')
for h in ['macos','linux']:
    rows=obj(h+'-latest-gates.json');require(len(rows)==5 and all(r['exit-code']==0 for r in rows if r['gate']!='integration'),h+' gate completion')
    require(all(r['exit-code']==0 for r in obj(h+'-owning-gates.json')),h+' owning completion')
require(obj('linux-final-integration.json')['exit-code']==0,'Linux final integration completion')
rows=obj('native-section-map-latest-gates.json');require(len(rows)==5 and all(r['exit-code']==(101 if r['gate']=='full' else 0) for r in rows),'Windows baseline gates')
for name in ['macos-final-collision-regression.log','linux-final-collision-regression.log','native-section-map-final-collision-regression.log']:require(results(name)==[[1,0,0]],'expanded native collision '+name)
def failures(raw):return [s.strip() for s in raw.split('failures:\n\n')[-1].splitlines() if s.startswith('    ')]
winfail=failures(text('native-section-map-latest-full.log'))
old=(repo/'docs/specifications/windows/native-section-open/native-section-open-final-full.log').read_bytes().decode('utf-8-sig').replace('\r\n','\n')
require(len(winfail)==5 and winfail==failures(old),'exact five baseline Windows failures')
freeze=obj('runtime-source-freeze.json');require(len(freeze)==20,'owned runtime source count')
for name,item in freeze.items():identity(repo/name,item)
archive_records={}
for h,name in [('macos','macos-owning-archive-identities.json'),('linux','linux-owning-archive-identities.json'),('windows','native-section-map-final-owning-archive-hashes.json')]:
    record=obj(name);require(record['core-features']==[] and len(record['sources'])==124,h+' core/source selection')
    for item in record['sources']:identity(repo/item['path'],item)
    require(len(record['archives'])==2 and all(r['members']>0 and r['structural-member-walk']=='pass' for r in record['archives']),h+' complete archive walk')
    archive_records[h]=record['archives']
windows=obj('native-section-map-final-owning-archive-hashes.json');require(len(windows['helpers'])==2,'two actual owning helpers')
for item in windows['helpers']:identity(base/item['path'],item)
consumer=obj('native-section-map-final-owning-source-verification.json');require(consumer['rust-source-units']==124 and len(consumer['cpp-consumers'])==9,'nine owning consumers')
for item in consumer['cpp-consumers']:identity(repo.parent.parent/item['path'],item)
require('100% tests passed out of 7' in text('macos-owning-ctest.log'),'seven owning Mac CTests')
for name in ['linux-owning-cpp.log','assist-native-section-map-map-owned-final-cpp.log']:
    raw=text(name);require('162 checks passed' in raw and '2/2 passed' in raw and 'PASS' in raw,'five C++ owning consumers '+name)
for name in ['macos-owning-protected-assist-cli.log','macos-owning-protected-assist-mcp.log']:require('0 failures' in text(name),'protected shipping executable '+name)
negative=obj('evidence-falsification-results.json');require(len(negative)==9 and all(r['rejected'] for r in negative),'nine independent corruptions rejected')
observer=obj('native-section-map-final-observer-identity.json');require(observer['bytes']==11177 and observer['sha256']=='d41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e' and observer['executable-machine']==0xAA64,'unchanged actual observer')
identity(base/'native-section-map-after-trace.rs',observer)
terminal=re.search(r'terminal turn=(\d+) Complete\(Internal\("([^"\n]+)"\)\)',text('native-section-map-final-after-production-trace-output.log'))
require(terminal is not None and int(terminal[1])==168227 and 'NtQueryPerformanceCounter (0x31)' in terminal[2],'actual ordinary continuation')
ordinary=text('native-section-map-final-after-ordinary-processes.log');require(ordinary.count('run reason=5 exit=0')==4,'four Internal outcomes')
frontiers=re.findall(r'native NTDLL service ([^"\n]+)',ordinary);require(len(frontiers)==4 and len(set(frontiers[:3]))==1 and 'NtQueryPerformanceCounter (0x31)' in frontiers[0] and 'NtTraceEvent (0x5e)' in frontiers[3],'ordinary frontier scope')
host=obj('native-section-map-final-host-identity.json');original=obj('native-section-map-fresh-identities.json')
require(host['same-as-original-native-captures'] and host['kernel']==original['kernel'] and host['principal']==original['principal'] and host['native-machine']==0xAA64,'final native host')
for now,prior in zip(host['libraries'],original['libraries']):require(all(now[k]==prior[k] for k in ['path','bytes','sha256','machine','characteristics']),'final library identity')
require(len(host['libraries'])==4,'four final libraries')
summary={'full-lib':full,'focused':focus,'capi':capi,'integration':integration,'windows-failures':winfail,'windows-baseline-failures':'exactly unchanged','all-targets':{h:0 for h in ['macos','linux','windows']},'controlled-falsifications':9,'map-replay-calls':4236,'native-matrix-observations':4632,'native-original-entry-cases':9,'whole-image-projection-bytes':6176768,'owning':{'rust-source-units':124,'matching-current-cpp-consumers':9,'helpers':2,'macos-ctests':7,'linux-cpp-consumers':5,'windows-cpp-consumers':5,'core-features':[],'whole-current-root-plugin-GUI-packages':'excluded','archives':archive_records},'final-test-only-delta':'AMD64 cache assertion; five focused Windows tests passed; shipping semantics unchanged; owning sources reverified','retained-failures':['macos C API30s native true timeout and repeat; final168passed','Linux integration543passed1timeout; isolated1passed and full544passed'],'ordinary':{'before-turn':161784,'after-turn':168227,'delta-scheduler-calls':6443,'observer-frontier':terminal[2],'cpp-frontiers':frontiers,'guest-exits':0},'full-same-host-goal':'active and incomplete'}
(base/'validation-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({'full-lib':full,'map-replay-calls':4236,'owning-source-units':124,'ordinary-turns':168227,'guest-exits':0}))
