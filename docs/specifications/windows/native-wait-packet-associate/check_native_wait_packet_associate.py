"""Validate native producers, original stdout, ownership identities and coverage."""
from pathlib import Path
import hashlib,json,re,sys

def require(ok,message):
    if not ok: raise ValueError(message)
require(not sys.flags.optimize,'optimized validation is rejected')
base=Path(sys.argv[1]) if len(sys.argv)>1 else Path(__file__).parent

def read(name):return (base/name).read_text(encoding='utf-8-sig')
def data(name):return json.loads(read(name))
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def rows(name,prefix):
    return [dict(field.split('=',1) for field in line.split()) for line in read(name).splitlines() if line.startswith(prefix)]
def statuses(name):
    return [tuple(map(int,m[1:])) for m in re.findall(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',read(name))]
manifest=data('evidence-manifest.json')
actual={p.name for p in base.iterdir() if p.is_file() and p.name!='evidence-manifest.json'}
require(actual==set(manifest),'exact evidence file coverage')
for name,item in manifest.items():
    p=base/name;require(p.stat().st_size==item['bytes'] and sha(p)==item['sha256'],'evidence identity '+name)
identity=data('native-wait-packet-association-producer-identities.json')
require(identity['principal']['sid']=='S-1-5-18' and identity['processor']=='ARM64','measured native principal/host')
require(len(identity['producer'])==72 and len(identity['sources'])==24 and len(identity['images'])==24,'24 original producer identities')
previous_dlls=data('before-ntdll-identities.json');require([x['sha256'] for x in identity['dlls']]==[previous_dlls['system32'],previous_dlls['syswow64']],'unchanged installed NTDLL identities')
profiles=['arm64','x64','x86','x86-laa'];families=['probe','validation','lifetime','lifetime-v2','order','boundaries']
expected={(family,profile,suffix) for family in families for profile in profiles for suffix in ['.exe','.log','-build.log']}
seen=set()
for item in identity['producer']:
    name=item['path'].replace('\\','/').split('/')[-1];stem='native-wait-packet-association-'+item['family']+'-'+item['profile'];suffix=name[len(stem):]
    key=(item['family'],item['profile'],suffix);require(key not in seen,'duplicate producer');seen.add(key)
    require(item['bytes']>=0 and re.fullmatch('[0-9A-Fa-f]{64}',item['sha256']),'producer dimensions')
    if suffix!='.exe':require((base/name).stat().st_size==item['bytes'] and sha(base/name)==item['sha256'].lower(),'actual original log '+name)
require(seen==expected,'complete producer matrix')
for item in identity['sources']:
    name=item['path'].replace('\\','/').split('/')[-1];require((base/name).stat().st_size==item['bytes'] and sha(base/name)==item['sha256'].lower(),'actual compiled producer source')
for item in identity['images']:
    require(item['machine']=={'arm64':0xAA64,'x64':0x8664,'x86':0x14C,'x86-laa':0x14C}[item['profile']],'actual PE architecture')
    if item['profile']=='x86-laa':require(item['characteristics']&0x20,'actual LAA image')
# Independently measured status tables, not derived from the guest implementation.
invalid=0xC0000008;wrong=0xC0000024;denied=0xC0000022
validation=[
 [0,invalid,invalid,wrong,wrong,denied,0,denied,denied,wrong,wrong],
 [0,invalid,invalid,wrong,wrong,denied,denied,0,denied,wrong,wrong],
 [0,invalid,invalid,0,denied,denied,denied,denied,0,0,0]]
counts={'validation':0,'transitions':0,'cancellation':0,'targets':0,'busy-order':0,'queued-boundaries':0}
for profile in profiles:
    prefix='native-wait-packet-association-';width=4 if profile.startswith('x86') else 8
    values=rows(prefix+'validation-'+profile+'.log','validation=');require(len(values)==135,'native validation count')
    require('complete validation=135 scenarios=2' in read(prefix+'validation-'+profile+'.log'),'native validation completion')
    for row in values:
        role,mode,pointer=map(int,[row['role'],row['mode'],row['pointer']]);status=int(row['status'],16)
        require(status==validation[role][mode] and int(row['exception'],16)==0,'native validation status')
        require(int(row['width'])==width and row['handles-before']==row['handles-after'],'validation width/lifetime')
        expected_protection=0x104 if pointer==4 and status else 2 if pointer==3 else 4
        require(int(row['protection'],16)==expected_protection,'one-shot guard/readonly result')
        for field,p in [('first',0),('unaligned',5),('last',6)]:
            require(int(row[field],16)==(0 if status==0 and pointer==p else 0xA5),'optional BOOLEAN byte')
    counts['validation']+=len(values)
    values=rows(prefix+'probe-'+profile+'.log','case=');require(len(values)==15 and 'complete cases=15' in read(prefix+'probe-'+profile+'.log'),'native transition completion')
    for row in values:
        require(row['status']=='00000000' and row['handles-before']==row['handles-after'],'transition status/lifetime')
        require(int(row['width'])==width and row['already']==('01' if row['signaled']=='1' else '00')+'A5'*15,'BOOLEAN width')
        for field in ['initial','associated','after-cancel','after-signal','after-remove','final-wait']:require(row[field]=='00000000','notification-signaled packet')
        if row['removed']=='00000000':
            mask=(1<<(width*8))-1
            for field,value in [('key',0x1234567887654321),('apc',0xA1B2C3D4CCCCDDDD),('information',0xFEDCBA9876543210)]:require(int(row[field],16)==value&mask,'full-width queued payload')
            require(row['io-status']=='C1234567','NT completion status')
        else:require(row['removed']=='00000102','empty completion port timeout')
    counts['transitions']+=len(values)
    name=prefix+'lifetime-v2-'+profile+'.log';values=rows(name,'cancel-mode=');require(len(values)==55 and 'complete lifetime=6 targets=13 cancel=55' in read(name),'native lifecycle completion')
    cancel_status=[0xC0000120,invalid,invalid,wrong,wrong,denied,0xC0000120,denied,denied,wrong,wrong]
    for row in values:require(int(row['status'],16)==cancel_status[int(row['cancel-mode'])] and row['handles-before']==row['handles-after'],'native cancellation status/lifetime')
    counts['cancellation']+=len(values)
    values=rows(name,'target-kind=');require(len(values)==13,'target count')
    target_status=[0,0,0,0,0xC00000F1,0,0,0,0,0xC00000F1,0xC00000F1,0,denied]
    for row in values:require(int(row['status'],16)==target_status[int(row['target-kind'])] and row['handles-before']==row['handles-after'],'target eligibility')
    counts['targets']+=len(values)
    name=prefix+'order-'+profile+'.log';values=rows(name,'order=');require(len(values)==20 and 'complete order=20 fifo=4' in read(name),'ordering completion')
    expected_order=[0,invalid,wrong,denied,invalid,wrong,denied,invalid,denied,0xC00000F1]
    for row in values:
        mode=int(row['order']);st=0xC00000EF if mode==0 and row['busy']=='1' else expected_order[mode]
        require(int(row['status'],16)==st and row['handles-before']==row['handles-after'],'busy validation order')
    counts['busy-order']+=len(values)
    fifo=rows(name,'fifo-kind=');require(len(fifo)==4,'four FIFO deliveries')
    for row in fifo:require(int(row['key'],16)==2-int(row['step']) and row['removed']=='00000000' and row['target-wait']=='00000102','FIFO signal consumption')
    name=prefix+'boundaries-'+profile+'.log';values=rows(name,'queued-flag=')
    require([int(row['queued-flag']) for row in values]==[0,1,2,127,128,255,256] and 'complete queued=7 dependencies=4' in read(name),'queued BOOLEAN boundaries')
    for row in values:
        boolean=int(row['queued-flag'])&255
        require(int(row['boolean'])==boolean and int(row['width'])==width and row['already']=='1','queued BOOLEAN width')
        require(row['associate']=='00000000' and int(row['cancel'],16)==(0 if boolean else 0x103) and int(row['removed'],16)==(0x102 if boolean else 0) and row['cancel-again']=='C0000120','queued BOOLEAN state transition')
        require(row['handles-before']==row['handles-after'],'queued BOOLEAN lifetime')
    counts['queued-boundaries']+=len(values)
    values=rows(name,'dependency-close=');require(len(values)==4,'dependency close boundaries')
    expected_close={(0,0):(0x103,0),(1,0):(0,0xC0000120),(0,1):(0xC0000120,0xC0000120),(1,1):(0x103,0)}
    require({(int(row['dependency-close']),int(row['initial'])) for row in values}==set(expected_close),'complete close state matrix')
    for row in values:
        key=(int(row['dependency-close']),int(row['initial']))
        require((int(row['cancel-false'],16),int(row['cancel-true'],16))==expected_close[key],'pending versus queued final port close')
        require(row['associate']==row['close']=='00000000' and int(row['already'])==key[1] and int(row['width'])==width and row['signal']==('00000000' if key[0]==0 else 'DEADBEEF') and row['handles-before']==row['handles-after'],'close boundary output/lifetime')
    counts['queued-boundaries']+=len(values)
summary=data('validation-summary.json');require(summary['replays']==dict(counts,total=sum(counts.values())) and sum(counts.values())==996,'replay coverage summary')
full={'macos': (7638, 0, 2), 'linux': (7629, 3, 2), 'windows': (7037, 5, 2)}
for host in ['macos','linux','windows']:
    prefix=host+'-' if host!='windows' else 'native-wait-packet-associate-'
    expected_tests=10 if host=='windows' else 9
    require(statuses(prefix+'targeted.log')==[(expected_tests,0,0)],'nonempty targeted '+host)
    require(statuses(prefix+'full.log')==[full[host]],'original full-suite '+host)
    require(summary['tests']['full'][host]==dict(zip(['pass','fail','ignored'],full[host])),'full stdout-bound summary')
    require(statuses(prefix+'capi.log')[0]==(168,0,0),'C API '+host)
    require(statuses(prefix+'integration.log')[0]==(4 if host=='windows' else 544,0,0),'registered integrations '+host)
    name=host+'-gates.json' if host!='windows' else 'native-wait-packet-associate-gates.json';gates=data(name)
    require(len(gates)==11 and {g['gate'] for g in gates}=={'targeted','wait-packet-create','threading','timer2','worker-factory','io-completion','events','full','capi','all-targets','integration'},'complete gate selection '+host)
    for g in gates:require(g['exit-code']==(101 if g['gate']=='full' and full[host][1] else 0),'gate exit '+host+'/'+g['gate'])
require(statuses('linux-timeout-isolated.log')==[(1,0,0)] and statuses('native-wait-packet-associate-clock-isolated.log')==[(1,0,0)],'unmodified isolated timing checks')
isolated=data('linux-final-isolated-gates.json')
require({x['test'] for x in isolated}=={'timers_do_not_outlive_exec_or_cross_fork','a_timeout_is_removed_or_updated','a_multishot_timeout_reports_each_expiry'} and len(isolated)==3,'all final timeout isolates')
for gate in isolated:require(gate['exit-code']==0 and '--exact' in gate['command'] and statuses('linux-final-isolated-'+gate['test']+'.log')==[(1,0,0)],'nonempty unchanged timeout isolate')
require('--test-threads=1' in next(x for x in data('linux-gates.json') if x['gate']=='integration')['command'],'final integration selection is serial')
hang=data('linux-integration-parallel-hang.json');require(hang['observed-completed']==58 and hang['cause']=='unknown','bounded stalled-run record')
require('running 544 tests' in read('linux-integration-parallel-hang.log') and 'rt_mutex_schedule' in read('linux-integration-parallel-host-threads.log'),'retained original stalled stdout and native threads')
require(next(x for x in data('linux-gates-before-serial-integration.json') if x['gate']=='integration')['exit-code']==137,'task-owned stalled runner termination retained')
require('100% tests passed out of 7' in read('macos-owning-ctest.log'),'owning CTest selection')
for host,number in [('macos',5),('linux',2)]:require(len(data(host+'-owning-gates.json'))==number and all(x['exit-code']==0 for x in data(host+'-owning-gates.json')),'owning command exits')
archive=data('native-wait-packet-associate-owning-archive-hashes.json')
require(archive['core-features']==[] and len(archive['sources'])==37 and len(archive['helpers'])==2,'native actual compiler ownership')
require(archive['sources']==data('owning-source-hashes.json')['sources'] and archive['helpers']==data('owning-source-hashes.json')['helpers'],'actual frozen compiler source identities')
source_root=Path(sys.argv[2]) if len(sys.argv)>2 else Path(__file__).resolve().parents[4]
for item in archive['sources']:
    path=source_root/item['path'];require(path.is_file() and path.stat().st_size==item['bytes'] and sha(path)==item['sha256'],'current source matches compiled source '+item['path'])

require(len(archive['archives'])==2 and all(x['structural-member-walk']=='pass' and x['bytes']>0 and x['members']>0 for x in archive['archives']),'complete native archive member walks')
source=data('native-wait-packet-associate-current-source-check.json');require(source['errors']==[] and source['source-files']==16583 and source['patches']==115,'complete applied native source check')
trace=read('native-wait-packet-associate-after-production-trace-output.log')
require('terminal turn=42564 Complete(Internal("native NTDLL service NtSetInformationWorkerFactory (0x1b1) at 0x180002b50 is not implemented"))' in trace,'actual new failure frontier')
require('terminal turn=42185 Complete(Internal("native NTDLL service NtAssociateWaitCompletionPacket (0x92) at 0x180001960 is not implemented"))' in read('before-production-trace.log'),'actual retained previous frontier')

require(sha(base/'native-wait-packet-associate-after-trace.rs')=='d41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e','identical observer')
ordinary=read('native-wait-packet-associate-after-ordinary-processes.log');require(ordinary.count('program ')==4 and ordinary.count('run reason=5 exit=0 diagnostic=')==4 and ordinary.count('NtSetInformationWorkerFactory (0x1b1)')==4,'four failed guest processes retained')
require(summary['frontier']['delta-scheduler-calls']==42564-42185 and summary['frontier']['guest-exit']=='not reached','bounded advancement summary')
require(summary['tests']['targeted']=={'macos':9,'linux':9,'windows':10},'targeted stdout-bound summary')
require(summary['tests']['capi']==dict.fromkeys(['macos','linux','windows'],168),'C API stdout-bound summary')
require(summary['tests']['integration']=={'macos':544,'linux':544,'windows':4},'integration stdout-bound summary')
require(summary['tests']['macos-ctest']==7,'CTest stdout-bound summary')
require(summary['source-overlay']=={'files':16583,'patches':115,'errors':0,'offline-adapter-bytes':374},'source-check-bound summary')
require(summary['native-owning']=={'source-hashes':37,'helper-hashes':2,'core-features':[]},'actual owning summary')
require(summary['frontier']['after']=={'turn':42564,'service':'NtSetInformationWorkerFactory','svc':'0x1b1','pc':'0x180002b50','arguments':['0x20','0x5','0xabe720','0x4']},'exact frontier summary')
for host,prefix in [('linux','linux-owning-cpp.log'),('windows','assist-native-wait-packet-associate-owned-cpp.log')]:
    text=read(prefix)
    for target in ['process_tool','process_tool_unavailable','rax_process','rax_abi_drift','rust_archive_link']:
        require(('run /validation/'+target+' /validation/'+target) in text if host=='linux' else (target+'.exe') in text,'actual C++ executable selection '+host+'/'+target)
require(summary['tests']['owning-cpp']==dict.fromkeys(['macos','linux','windows'],5),'owning C++ coverage summary')

print('pass: 24 producer identities; 996 native row replays; three host gate sets; owning archives; guest failure frontier retained')
