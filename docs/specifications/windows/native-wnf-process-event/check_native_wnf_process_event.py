"""Validate native process notification registration captures and producer identity."""
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
identity=data('native-wnf-process-event-producer-identities.json')
profiles=['arm64','x64','x86','x86-laa']
expected_profiles={('event',p) for p in profiles}|{('event-fresh','arm64'),('status','arm64')}
require(identity['processor']=='ARM64' and identity['principal']['sid']=='S-1-5-18','actual host/principal')
previous=data('before-ntdll-identities.json')
require([x['sha256'] for x in identity['dlls']]==[previous['system32'],previous['syswow64']],'unchanged installed NTDLL')
def stem(family,profile):return ('native-wnf-status' if family=='status' else 'native-wnf-process-'+family)+'-'+profile
expected={(family,profile,suffix) for family,profile in expected_profiles for suffix in ['.exe','.log','-build.log']};seen=set()
for row in identity['producer']:
    name=row['path'].replace('\\','/').split('/')[-1];prefix=stem(row['family'],row['profile']);suffix=name[len(prefix):];key=(row['family'],row['profile'],suffix)
    require(name.startswith(prefix) and key in expected and key not in seen,'unique actual producer matrix');seen.add(key)
    require(re.fullmatch('[0-9A-Fa-f]{64}',row['sha256']) and row['bytes']>0,'producer dimensions')
    if suffix!='.exe':require((base/name).stat().st_size==row['bytes'] and sha(base/name)==row['sha256'].lower(),'actual producer log')
require(seen==expected,'complete six native producers');seen=set()
for row in identity['sources']:
    key=(row['family'],row['profile']);require(key in expected_profiles and key not in seen,'unique source profile');seen.add(key)
    name=row['path'].replace('\\','/').split('/')[-1];expected_name='native-wnf-status.cpp' if row['family']=='status' else 'native-wnf-process-'+row['family']+'.cpp'
    require(name==expected_name and (base/name).stat().st_size==row['bytes'] and sha(base/name)==row['sha256'].lower(),'actual compiled source')
require(seen==expected_profiles,'complete source identities');seen=set()
for row in identity['images']:
    key=(row['family'],row['profile']);require(key in expected_profiles and key not in seen,'unique actual image');seen.add(key)
    require(row['machine']=={'arm64':0xAA64,'x64':0x8664,'x86':0x14C,'x86-laa':0x14C}[row['profile']],'actual PE architecture')
    if row['profile']=='x86-laa':require(row['characteristics']&0x20,'actual LAA bit')
require(seen==expected_profiles,'complete PE identities')

def rows(name):return [dict(x.split('=',1) for x in line.split()) for line in read(name).splitlines() if line.startswith('case=')]
def state(row,width,manual,initial):
    require(int(row['width'])==width and row['query-before']==row['query-after']=='00000000','event queries/width')
    require(row['returned-before']==row['returned-after']=='8','exact EventBasic8 B')
    require(int(row['type-before'])==int(row['type-after'])==(0 if manual else 1),'unchanged event type')
    require(int(row['state-before'])==int(row['state-after'])==initial,'unchanged signal state')
    require(row['handles-before']==row['handles-after'],'parent handle lifetime')
status=[0xC0000718,0xC0000008,0xC0000008,0xC0000024,0xC0000024,0xC0000022,0xC0000022,0,0,0xC0000022,0,0xC0000024]
status[7]=status[8]=status[10]=0xC0000718
cases=[(manual,initial,mode) for manual in [0,1] for initial in [0,1] for mode in range(12)]
total=0
for profile in profiles:
    name='native-wnf-process-event-'+profile+'.log';observed=rows(name)
    require(len(observed)==48 and [int(r['case']) for r in observed]==list(range(48)) and 'complete cases=48' in read(name),'exact initialized matrix')
    require([(int(r['manual']),int(r['initial']),int(r['mode'])) for r in observed]==cases,'all initialized partitions')
    for r in observed:
        mode=int(r['mode']);require(int(r['status'],16)==int(r['repeat'],16)==status[mode],'initialized typed/grant/busy precedence')
        require(r['reset']==r['clear']=='C0000008','NULL never unregisters')
        state(r,4 if profile.startswith('x86') else 8,int(r['manual']),int(r['initial']))
    total+=len(observed)
name='native-wnf-process-event-fresh-arm64.log';observed=rows(name)
require(len(observed)==49 and [int(r['case']) for r in observed]==list(range(49)) and observed[0]['warmup']=='1' and observed[0]['mode']=='12','one explicit debugger warmup')
require('complete cases=48 warmup=1' in read(name),'complete fresh capture')
require(observed[0]['status']=='00000000' and observed[0]['repeat']==observed[0]['replacement']==observed[0]['replacement-repeat']=='C0000718','original RTL Event first/closed-handle registration')
require([(int(r['manual']),int(r['initial']),int(r['mode'])) for r in observed[1:]]==cases,'all fresh partitions')
for r in observed[1:]:
    mode=int(r['mode']);first=0 if status[mode]==0xC0000718 else status[mode]
    require(r['warmup']=='0' and r['installed']==r['entered']=='1' and r['calls']=='4' and r['exit']=='77','actual original syscall capture/owned child termination')
    require(int(r['status'],16)==first and int(r['repeat'],16)==(0xC0000718 if first==0 else first),'first success/repeat validation')
    require(int(r['replacement'],16)==(0xC0000718 if first==0 else 0) and r['replacement-repeat']=='C0000718','close cannot clear; failures cannot register')
    state(r,8,int(r['manual']),int(r['initial']))
objects=[dict(x.split('=',1) for x in line.split()[1:]) for line in read(name).splitlines() if line.startswith('object ')]
require(len(objects)==196 and all(r['status']=='00000000' and r['returned']=='56' for r in objects),'all exact ObjectBasic56 B queries')
traps=[dict(x.split('=',1) for x in line.split()[1:]) for line in read(name).splitlines() if line.startswith('trap ')]
require(len(traps)==245 and all(r['exception']=='80000003' for r in traps),'five verified ARM64 breakpoint sites per child')
total+=48;require(total==240,'exact successful registration replay rows')
observed=rows('native-wnf-status-arm64.log');require([(int(r['status'],16),int(r['error'])) for r in observed]==[(0,0),(0xC0000008,6),(0xC0000024,6),(0xC0000022,5),(0xC0000718,1359)],'actual RTL error mapping')
require('complete cases=5' in read('native-wnf-status-arm64.log'),'complete native status producer')
prefix="native-wnf-process-event-"
EXPECTED_FULL={'macos': {'pass': 7660, 'fail': 0, 'ignored': 2}, 'linux': {'pass': 7652, 'fail': 2, 'ignored': 2}, 'windows': {'pass': 7063, 'fail': 5, 'ignored': 2}}
EXPECTED_FRONTIER={'before-turn': 44258, 'after-turn': 46245, 'delta-scheduler-calls': 1987, 'service': 'NtSubscribeWnfStateChange', 'svc': '0x1cd', 'pc': '0x180002d10', 'thread': 8, 'arguments': ['0x11037c0', '0x0', '0x11', '0xabe918'], 'guest-exit': 'not reached'}
EXPECTED_LINUX_FAILURES=['a_timeout_is_removed_or_updated','a_multishot_timeout_reports_each_expiry']
summary=data('validation-summary.json');require(summary['replays']==240,'stdout-bound replay summary')
require(summary['tests']['full']==EXPECTED_FULL,'frozen full-suite counts')
required={'targeted','binding','soft-maximum','hard-maximum','wait-packet-associate','wait-packet-create','threading','timer2','worker-factory','io-completion','events','full','capi','all-targets','integration'}
for host in ['macos','linux','windows']:
    stem=host+'-' if host!='windows' else prefix
    gates=data(host+'-gates.json' if host!='windows' else prefix+'gates.json')
    require(len(gates)==15 and {x['gate'] for x in gates}==required,'complete host gate selection '+host)
    counts=summary['tests']['full'][host];full=tuple(counts[x] for x in ['pass','fail','ignored'])
    require(results(stem+'full.log')==[full],'original full stdout '+host)
    for gate in gates:require(gate['exit-code']==(101 if gate['gate']=='full' and full[1] else 0),'gate exit '+host+'/'+gate['gate'])
    require(results(stem+'targeted.log')==[(8 if host=='windows' else 7,0,0)],'nonempty targeted gate '+host)
    require(results(stem+'hard-maximum.log')==[(5 if host=='windows' else 4,0,0)],'hard-maximum regression '+host)
    require(results(stem+'soft-maximum.log')==[(6 if host=='windows' else 5,0,0)],'soft-maximum regression '+host)
    require(results(stem+'capi.log')[0]==(168,0,0),'C API '+host)
    require(results(stem+'integration.log')[0]==(4 if host=='windows' else 544,0,0),'registered integration '+host)
    require(summary['tests']['targeted'][host]==(8 if host=='windows' else 7),'targeted stdout-bound summary')
require('--test-threads=1' in next(x for x in data('linux-gates.json') if x['gate']=='integration')['command'],'Linux integration is serial')
require('100% tests passed out of 7' in read('macos-owning-ctest.log'),'owning CTest selection')
for host,count in [('macos',5),('linux',2)]:require(len(data(host+'-owning-gates.json'))==count and all(x['exit-code']==0 for x in data(host+'-owning-gates.json')),'owning exits '+host)
archive=data(prefix+'owning-archive-hashes.json');frozen=data('owning-source-hashes.json')
require(archive['core-features']==[] and len(archive['sources'])==47 and len(archive['helpers'])==2,'actual compiler source/core-feature ownership')
require(archive['sources']==frozen['sources'] and archive['helpers']==frozen['helpers'],'frozen source/helper identities')
for row in archive['sources']:
    p=source_root/row['path'];require(p.is_file() and p.stat().st_size==row['bytes'] and sha(p)==row['sha256'],'current source matches compiled snapshot '+row['path'])
require(len(archive['archives'])==2 and all(x['structural-member-walk']=='pass' and x['members']>0 and x['bytes']>0 for x in archive['archives']),'complete native archive walks')
applied=data(prefix+'current-source-check.json');require(applied['errors']==[] and applied['source-files']==17118 and applied['patches']==91,'complete applied native source snapshot')
require(read('root-owning-source-delta.txt').splitlines()==['VENDORED_VERSIONS.md','src/emulation/native-process-emulation.md'],'frozen Unix owning root code parity')
require(sha(base/(prefix+'after-trace.rs'))=='d41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e','identical process observer')
require('terminal turn=44258 Complete(Internal(' in read('before-production-trace.log') and 'NtSetWnfProcessNotificationEvent' in read('before-production-trace.log'),'actual previous WNF frontier')
trace=read(prefix+'after-production-trace-output.log')
require('registers=[11037c0, 0, 11, abe918,' in trace and 'NtSubscribeWnfStateChange (0x1cd) at 0x180002d10 is not implemented' in trace,'actual next service/formal arguments')
frontier=summary['frontier']
require(frontier==EXPECTED_FRONTIER,'bounded frontier summary')
require('terminal turn='+str(frontier['after-turn'])+' Complete(Internal(' in trace and frontier['service'] in trace,'actual terminal frontier')
require('turn='+str(frontier['after-turn'])+' thread='+str(frontier['thread'])+' PC='+frontier['pc'] in trace,'actual service PC/thread')
require(frontier['after-turn']-frontier['before-turn']==frontier['delta-scheduler-calls'],'scheduler-call delta')
ordinary=read(prefix+'after-ordinary-processes.log')
require(ordinary.count('program ')==4 and ordinary.count('run reason=5 exit=0 diagnostic=')==4 and ordinary.count(frontier['service'])>=4,'four guest failures retained')
isolated=data('linux-isolated-gates.json')
require(len(isolated)==len(EXPECTED_LINUX_FAILURES) and {x['test'] for x in isolated}==set(EXPECTED_LINUX_FAILURES),'all failed unchanged Linux timeout isolates')
for gate in isolated:require(gate['exit-code']==0 and '--exact' in gate['command'] and results('linux-isolated-'+gate['test']+'.log')==[(1,0,0)],'nonempty unchanged timeout isolate')
require(results('macos-baseline-red.log')==[(0,5,0)] and 'NtSetWnfProcessNotificationEvent' in read('macos-baseline-red.log'),'observed pre-fix red baseline')
for host,name in [('linux','linux-owning-cpp.log'),('windows','assist-native-wnf-process-event-owned-cpp.log')]:
    text=read(name)
    for target in ['process_tool','process_tool_unavailable','rax_process','rax_abi_drift','rust_archive_link']:
        require(('run /validation/'+target+' /validation/'+target) in text if host=='linux' else target+'.exe' in text,'actual C++ consumer '+host+'/'+target)
print('pass: six native producers; 240 registration replays and5 translations; all host gate sets; owning/source/archive identities')

for host,prefix in [('macos','macos-final-'),('windows','native-wnf-process-event-final-')]:
    gates=data(prefix+'gates.json');require(len(gates)==2 and {x['gate'] for x in gates}=={'targeted','all-targets'} and all(x['exit-code']==0 for x in gates),'final formatted source compile '+host)
    require(results(prefix+'targeted.log')==[(8 if host=='windows' else 7,0,0)],'final unchanged service/test semantics '+host)
proof=data('final-source-formatting-proof.json');require(len(proof)==2 and all(x['noncomment-code-and-independent-module-set']=='identical' for x in proof),'format/comment-only reconciliation')
