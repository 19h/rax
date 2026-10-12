"""Bind independent native maximum-setting captures to source and build evidence."""
from pathlib import Path
import hashlib,json,re,sys

def require(ok,message):
    if not ok:raise ValueError(message)
require(not sys.flags.optimize,'optimized validation is rejected')
base=Path(sys.argv[1]) if len(sys.argv)>1 else Path(__file__).parent
source_root=Path(sys.argv[2]) if len(sys.argv)>2 else Path(__file__).resolve().parents[4]
def read(name):return (base/name).read_text(encoding='utf-8-sig')
def data(name):return json.loads(read(name))
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def results(name):return [tuple(map(int,m)) for m in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',read(name))]
manifest=data('evidence-manifest.json')
require(set(manifest)=={p.name for p in base.iterdir() if p.is_file() and p.name!='evidence-manifest.json'},'exact evidence coverage')
for name,row in manifest.items():require((base/name).stat().st_size==row['bytes'] and sha(base/name)==row['sha256'],'evidence identity '+name)
identity=data('native-worker-factory-set-producer-identities.json')
require(identity['processor']=='ARM64' and identity['principal']['sid']=='S-1-5-18','native host/caller identity')
require(len(identity['producer'])==12 and len(identity['sources'])==len(identity['images'])==4,'four actual producer identities')
previous=data('before-ntdll-identities.json')
require([x['sha256'] for x in identity['dlls']]==[previous['system32'],previous['syswow64']],'unchanged installed NTDLL identities')
profiles=['arm64','x64','x86','x86-laa'];prefix='native-worker-factory-set-maximum-'
expected={(profile,suffix) for profile in profiles for suffix in ['.exe','.log','-build.log']};seen=set()
for row in identity['producer']:
    name=row['path'].replace('\\','/').split('/')[-1];stem=prefix+row['profile'];suffix=name[len(stem):]
    key=(row['profile'],suffix);require(key not in seen,'duplicate producer');seen.add(key)
    require(re.fullmatch('[0-9A-Fa-f]{64}',row['sha256']) and row['bytes']>=0,'producer dimensions')
    if suffix!='.exe':require((base/name).stat().st_size==row['bytes'] and sha(base/name)==row['sha256'].lower(),'original producer log '+name)
require(seen==expected,'complete producer matrix')
for row in identity['sources']:
    name=row['path'].replace('\\','/').split('/')[-1]
    require((base/name).stat().st_size==row['bytes'] and sha(base/name)==row['sha256'].lower(),'actual compiled producer source')
for row in identity['images']:
    require(row['machine']=={'arm64':0xAA64,'x64':0x8664,'x86':0x14C,'x86-laa':0x14C}[row['profile']],'actual PE architecture')
    if row['profile']=='x86-laa':require(row['characteristics']&0x20,'actual LAA image')
# Status constants and order come from these independent kernel measurements.
handle_status=[0,0xC0000008,0xC0000008,0xC0000024,0xC0000024,0xC0000022,0xC0000022,0xC0000022,0,0xC0000022,0,0xC0000024,0xC0000024]
total=0
for profile in profiles:
    name=prefix+profile+'.log';rows=[dict(x.split('=',1) for x in line.split()) for line in read(name).splitlines() if line.startswith('case=')]
    require(len(rows)==239 and [int(x['case']) for x in rows]==list(range(239)) and 'complete cases=239' in read(name),'complete native case coverage')
    cases=[('value',value,0,4,0) for value in [0,1,2,511,512,513,768,1024,0x7FFFFFFF,0x80000000,0xFFFFFFFF]]
    cases += [('buffer',768,mode,length,0) for mode in range(9) for length in [0,1,3,4,5,8,128,0xFFFFFFFF]]
    cases += [('handle-order',768,mode,length,handle) for handle in range(13) for mode in [0,1,4,5] for length in [0,4,5]]
    require([(x['label'],int(x['value'],16),int(x['mode']),int(x['length'],16),int(x['handle-mode'])) for x in rows]==cases,'exact independent case matrix')
    width=4 if profile.startswith('x86') else 8;size=96 if width==4 else 120
    for row in rows:
        mode=int(row['mode']);length=int(row['length'],16);handle=int(row['handle-mode']);value=int(row['value'],16)
        status=0xC0000004 if length!=4 else 0xC0000005 if mode in [1,2,4,8] else 0x80000001 if mode==5 else handle_status[handle]
        require(int(row['status'],16)==status and row['exception']=='00000000' and row['class']=='5','measured length/capture/type/access order')
        require(int(row['width'])==width and row['query-before']==row['query-after']=='00000000' and int(row['returned-before'])==int(row['returned-after'])==size,'successful native query layout')
        require(int(row['maximum-before'],16)==512 and int(row['maximum-after'],16)==(value if status==0 else 512),'unsigned maximum state/no partial mutation')
        require(row['total-before']==row['total-after']=='0' and row['handles-before']==row['handles-after'],'dormant worker/handle lifetime')
        protection=2 if mode==3 else 1 if mode==4 else 0x104 if mode==5 and length!=4 else 4
        require(int(row['protection'],16)==protection,'one-shot guard/readonly state')
        for field,maximum in [('before',512),('after',value if status==0 else 512)]:
            raw=bytes.fromhex(row[field]);require(len(raw)==128 and int.from_bytes(raw[40:44],'little')==maximum and raw[size:]==b'\xA5'*(128-size),'actual query bytes and untouched tail')
    total+=len(rows)
require(total==956,'exact native replay total')
summary=data('validation-summary.json');require(summary['replays']==956,'stdout-bound replay summary')
require(summary['tests']['full']=={'macos':{'pass':7642,'fail':0,'ignored':2},'linux':{'pass':7634,'fail':2,'ignored':2},'windows':{'pass':7042,'fail':5,'ignored':2}},'frozen full-suite counts')
required={'targeted','wait-packet-associate','wait-packet-create','threading','timer2','worker-factory','io-completion','events','full','capi','all-targets','integration'}
for host in ['macos','linux','windows']:
    stem=host+'-' if host!='windows' else prefix
    gates=data(host+'-gates.json' if host!='windows' else prefix+'gates.json')
    require(len(gates)==12 and {x['gate'] for x in gates}==required,'complete host gate selection '+host)
    counts=summary['tests']['full'][host];full=tuple(counts[x] for x in ['pass','fail','ignored'])
    require(results(stem+'full.log')==[full],'original full stdout '+host)
    for gate in gates:require(gate['exit-code']==(101 if gate['gate']=='full' and full[1] else 0),'gate exit '+host+'/'+gate['gate'])
    require(results(stem+'targeted.log')==[(5 if host=='windows' else 4,0,0)],'nonempty targeted gate '+host)
    require(results(stem+'capi.log')[0]==(168,0,0),'C API '+host)
    require(results(stem+'integration.log')[0]==(4 if host=='windows' else 544,0,0),'registered integration '+host)
    require(summary['tests']['targeted'][host]==(5 if host=='windows' else 4),'targeted stdout-bound summary')
require('--test-threads=1' in next(x for x in data('linux-gates.json') if x['gate']=='integration')['command'],'Linux integration is serial')
require('100% tests passed out of 7' in read('macos-owning-ctest.log'),'owning CTest selection')
for host,count in [('macos',5),('linux',2)]:require(len(data(host+'-owning-gates.json'))==count and all(x['exit-code']==0 for x in data(host+'-owning-gates.json')),'owning exits '+host)
archive=data(prefix+'owning-archive-hashes.json');frozen=data('owning-source-hashes.json')
require(archive['core-features']==[] and len(archive['sources'])==39 and len(archive['helpers'])==2,'actual compiler source/core-feature ownership')
require(archive['sources']==frozen['sources'] and archive['helpers']==frozen['helpers'],'frozen source/helper identities')
for row in archive['sources']:
    p=source_root/row['path'];require(p.is_file() and p.stat().st_size==row['bytes'] and sha(p)==row['sha256'],'current source matches compiled snapshot '+row['path'])
require(len(archive['archives'])==2 and all(x['structural-member-walk']=='pass' and x['members']>0 and x['bytes']>0 for x in archive['archives']),'complete native archive walks')
applied=data(prefix+'current-source-check.json');require(applied['errors']==[] and applied['source-files']==16618 and applied['patches']==100,'complete applied native source snapshot')
require(read('root-owning-source-delta.txt').splitlines()==['VENDORED_VERSIONS.md','src/emulation/native-process-emulation.md'],'frozen Unix owning root code parity')
require(sha(base/(prefix+'after-trace.rs'))=='d41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e','identical process observer')
require('terminal turn=42564 Complete(Internal("native NTDLL service NtSetInformationWorkerFactory (0x1b1) at 0x180002b50 is not implemented"))' in read('before-production-trace.log'),'actual previous frontier')
trace=read(prefix+'after-production-trace-output.log')
require('terminal turn=42609 Complete(Internal(' in trace and 'NtSetInformationWorkerFactory class 14' in trace and 'turn=42609 thread=8 PC=0x180002b50' in trace and 'registers=[20, e, abe730, 4,' in trace,'actual class14 frontier')
ordinary=read(prefix+'after-ordinary-processes.log')
require(ordinary.count('program ')==4 and ordinary.count('run reason=5 exit=0 diagnostic=')==4 and ordinary.count('NtSetInformationWorkerFactory class 14')==4,'four guest failures retained')
require(summary['frontier']=={'before-turn':42564,'after-turn':42609,'delta-scheduler-calls':45,'service':'NtSetInformationWorkerFactory','svc':'0x1b1','pc':'0x180002b50','class':14,'arguments':['0x20','0xe','0xabe730','0x4'],'guest-exit':'not reached'},'bounded frontier summary')
isolated=data('linux-isolated-gates.json');require(len(isolated)==2 and {x['test'] for x in isolated}=={'a_timeout_is_removed_or_updated','a_multishot_timeout_reports_each_expiry'},'all failed timeout isolates')
for gate in isolated:require(gate['exit-code']==0 and '--exact' in gate['command'] and results('linux-isolated-'+gate['test']+'.log')==[(1,0,0)],'nonempty unchanged timeout isolate')
for host,name in [('linux','linux-owning-cpp.log'),('windows','assist-native-worker-factory-set-maximum-owned-cpp.log')]:
    text=read(name)
    for target in ['process_tool','process_tool_unavailable','rax_process','rax_abi_drift','rust_archive_link']:
        require(('run /validation/'+target+' /validation/'+target) in text if host=='linux' else target+'.exe' in text,'actual C++ consumer '+host+'/'+target)
print('pass: four native producers; 956 exact replays; all host gate sets; owning/source/archive identities')
