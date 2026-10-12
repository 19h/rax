"""Validate measured ETW function27, exact producer and owning-build evidence."""
from pathlib import Path
import hashlib,itertools,json,re,sys

def require(ok,message):
    if not ok:raise ValueError(message)
require(not sys.flags.optimize,'optimized validation rejected')
base=Path(sys.argv[1]) if len(sys.argv)>1 else Path(__file__).parent
source_root=Path(sys.argv[2]) if len(sys.argv)>2 else Path(__file__).resolve().parents[4]
def read(name):return (base/name).read_text(encoding='utf-8-sig')
def data(name):return json.loads(read(name))
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def rows(name,prefix='case='):
    return [dict(f.split('=',1) for f in line.split() if '=' in f) for line in read(name).splitlines() if line.startswith(prefix)]
def num(row,key,radix=16):return int(row[key],radix)
def results(name):return [tuple(map(int,x)) for x in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',read(name))]
manifest=data('evidence-manifest.json')
require(set(manifest)=={p.name for p in base.iterdir() if p.is_file() and p.name!='evidence-manifest.json'},'exact evidence coverage')
for name,r in manifest.items():require((base/name).stat().st_size==r['bytes'] and sha(base/name)==r['sha256'],'evidence identity '+name)
profiles=['arm64','x64','x86','x86-laa']
expected={'native-etw-notification-'+family+'-'+profile for family in ['event','boundaries','output-readonly'] for profile in profiles}
expected|={'native-etw-notification-'+family+'-arm64' for family in ['event-fresh','return-fresh','alias-fresh']}
identity=data('native-etw-notification-producer-identities.json')
require(identity['windows-version']=='10.0.29683' and identity['host-machine'].upper()=='ARM64','installed host identity')
require(len(identity['producers'])==15 and {r['profile'] for r in identity['producers']}==expected,'exact native producers')
for r in identity['producers']:
    s,l,e=r['source_identity'],r['log_identity'],r['executable_identity']
    require(s['sha256']==r['source-sha256']==sha(base/r['source']) and s['bytes']==(base/r['source']).stat().st_size,'actual compiled producer source')
    require(l['sha256']==sha(base/r['log']) and l['bytes']==(base/r['log']).stat().st_size,'actual producer stdout')
    profile=next(p for p in sorted(profiles,key=len,reverse=True) if r['profile'].endswith('-'+p))
    require(e['machine']=={'arm64':0xAA64,'x64':0x8664,'x86':0x14C,'x86-laa':0x14C}[profile],'actual PE architecture')
    require(e['bytes']>0 and re.fullmatch('[a-f0-9]{64}',e['sha256']),'actual PE identity')
    if profile=='x86-laa':require(e['characteristics']&0x20,'PE LAA flag')
require([r['sha256'] for r in identity['native-libraries']]==['502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f','ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e'],'pinned NTDLL identities')
require(sha(base/'phnt-ntwmi.h')=='ea487f6366b9678ab31a2f8509b4fbc0be63d4deb83a86b123da83c66b21cf21','pinned PHNT primary source')
INVALID,HANDLE,TYPE,DENIED,BUSY,AV,GUARD=0xC000000D,0xC0000008,0xC0000024,0xC0000022,0xC0000718,0xC0000005,0x80000001
POISON=0xA5A5A5A5
def handle_status(mode,busy=False):return INVALID if mode==1 else HANDLE if mode in [2,3,4] else TYPE if mode==11 else DENIED if mode in [5,6,9] else BUSY if busy else 0
total=0
for profile in profiles:
    stem='native-etw-notification-event-'+profile+'.log';observed=rows(stem)
    partition=list(itertools.product(range(2),range(2),range(12),[0,1,2,3,4,5,7,8,9,16]))
    require(len(observed)==480 and [num(r,'case',10) for r in observed]==list(range(480)) and [(num(r,'manual',10),num(r,'initial',10),num(r,'mode',10),num(r,'length',10)) for r in observed]==partition,'exact mature handle partitions')
    require('complete cases=480' in read(stem),'complete mature handle run')
    for r in observed:
        status=handle_status(num(r,'mode',10),True) if num(r,'length',10)==4 else INVALID
        require(num(r,'status')==num(r,'repeat')==status,'typed access length status')
        require(num(r,'returned')==num(r,'repeat-returned')==POISON and num(r,'canary')==0xB6B6B6B6,'failure output unchanged')
        require(r['handles-before']==r['handles-after'],'mature handles restored')
        require(r['query-before']==r['query-after']=='00000000' and r['type-before']==r['type-after'] and r['state-before']==r['state-after'],'Event state unchanged')
    total+=480
    for family,outputs,count in [('boundaries',range(4),4200),('output-readonly',[4],1050)]:
        stem='native-etw-notification-'+family+'-'+profile+'.log';observed=rows(stem)
        partition=list(itertools.product(range(10),range(7),outputs,[0,3,4,5,8],[0,4,0xFFFFFFFF]))
        require(len(observed)==count and [(num(r,'input',10),num(r,'returned',10),num(r,'output',10),num(r,'input-length',10),num(r,'output-length',10)) for r in observed]==partition,'exact buffer partitions')
        require([num(r,'case',10) for r in observed]==list(range(count)) and f'complete cases={count}' in read(stem),'complete unique buffer ordinals')
        for r in observed:
            ik,rk,ok,il,ol=(num(r,k,10) for k in ['input','returned','output','input-length','output-length'])
            ip=0x104 if ik==3 else 1 if ik==4 else 2 if ik==5 else 4
            rp=0x104 if rk==3 else 1 if rk==4 else 2 if rk==5 else 4
            up=1 if rk==6 else 4;op=1 if ok==2 else 0x104 if ok==3 else 2 if ok==4 else 4
            if ol and ok in [2,3,4]:
                status=GUARD if ok==3 else AV
                if ok==3:op=4
            elif ol==0xFFFFFFFF and ok==1:status=AV
            elif rk==1:status=INVALID
            elif rk in [2,4,5,6]:status=AV
            elif rk==3:status=GUARD;rp=4
            elif il!=4 or ik==1:status=INVALID
            elif ik in [2,4,6]:status=AV
            elif ik==3:status=GUARD;ip=4
            else:status=BUSY
            require(num(r,'status')==status,'buffer fault precedence')
            require(tuple(num(r,k) for k in ['input-protect','returned-protect','returned-upper-protect','output-protect'])==(ip,rp,up,op),'first reached guard consumption')
            require(num(r,'return-after')==num(r,'output-after')==POISON,'no failure output mutation')
        total+=count
require(total==22920,'mature complete count')
for family,count in [('event-fresh',48),('return-fresh',120),('alias-fresh',72)]:
    stem='native-etw-notification-'+family+'-arm64.log';observed=rows(stem);returned=rows(stem,'trace-returned');traps=rows(stem,'trap ')
    require(len(observed)==len(returned)==count+1 and [num(r,'case',10) for r in observed]==list(range(count+1)),'fresh unique ordinals')
    require(observed[0]['warmup']=='1' and all(r['warmup']=='0' for r in observed[1:]) and f'complete cases={count} warmup=1' in read(stem),'excluded fresh warmup')
    require(len(traps)==5*(count+1),'actual entry and four true returns')
    for ordinal in range(count+1):require([r['phase'] for r in traps if num(r,'case',10)==ordinal]==['entry','return-0','return-1','return-2','return-3'],'breakpoint sequence')
    for r,o in zip(observed[1:],returned[1:]):
        require(r['case']==o['case'] and r['calls']=='4' and r['exit']=='77' and r['installed']==r['entered']=='1','actual native child completion')
        require(r['handles-before']==r['handles-after'] and r['query-before']==r['query-after']=='00000000','fresh resource lifetime')
        mode=num(r,'mode',10);first=handle_status(mode)
        if family=='return-fresh':
            kind=num(o,'kind',10);first=INVALID if kind==1 else AV if kind in [2,3,4,6,7] else GUARD if kind in [5,9] else first
            require(num(o,'protect')==(4 if kind not in [3,4,6,7] else 1 if kind in [3,6] else 2),'fresh return guard/protection')
        second=handle_status(mode,first==0)
        third=BUSY if first==0 or second==0 else 0
        require(tuple(num(r,k) for k in ['status','repeat','replacement','replacement-repeat'])==(first,second,third,BUSY),'first probe publication close replacement')
        require(r['type-before']==r['type-after'] and r['state-before']==r['state-after'],'fresh Event type/signal unchanged')
        expected_a=0 if first==0 else POISON
        if family=='alias-fresh' and num(o,'kind',10)<=3 and first!=0:
            memory=(num(r,'selected')&0xFFFFFFFF).to_bytes(4,'little')+b'\xA5'*8;offset=num(o,'kind',10);expected_a=int.from_bytes(memory[offset:offset+4],'little')
        require(tuple(num(o,k) for k in ['a','b','c','d'])==(expected_a,0 if second==0 else POISON,0 if third==0 else POISON,POISON),'exact first output capture/store and failures')
    total+=count
require(total==23160,'all complete native cases')
require(results('diagnostic-windows-capi-parallel.log')==[(166,2,0)],'retained first parallel C API failures')
require('--test-threads=1' in next(r['command'] for r in data('native-etw-notification-gates.json') if r['gate']=='capi'),'complete serial Windows C API retry')
require(data('windows-owning-exit.json')['exit-code']==0,'actual owning Windows command completion')
raw=data('diagnostic-windows-gates-raw-recovery.json');prior=data('diagnostic-windows-gates-before-capi-retry.json');recovery=data('native-etw-notification-recovery-gates.json')
require(raw[0]['value']==prior and raw[0]['Count']==15 and raw[1:]==recovery,'raw PowerShell array and recovery provenance')
require(data('native-etw-notification-gates.json')==[r for r in prior if r['gate']!='capi']+recovery,'explicit logical gate normalization')
require([r['gate'] for r in recovery]==['capi','all-targets','integration'] and all(r['exit-code']==0 for r in recovery),'actual final recovery commands')
summary=data('validation-summary.json')
require(summary['native-cases']==23160 and summary['native-warmups-excluded']==3,'native summary counts')
prefix='native-etw-notification-'
required={'targeted','previous-wnf-subscription','previous-wnf-event','binding','soft-maximum','hard-maximum','wait-packet-associate','wait-packet-create','threading','timer2','worker-factory','io-completion','events','full','capi','all-targets','integration'}
for host in ['macos','linux','windows']:
    stem=host+'-' if host!='windows' else prefix
    gates=data(host+'-gates.json' if host!='windows' else prefix+'gates.json')
    require(len(gates)==17 and {r['gate'] for r in gates}==required,'seventeen host gates '+host)
    full=summary['tests']['full'][host];counts=tuple(full[k] for k in ['pass','fail','ignored'])
    require(results(stem+'full.log')==[counts] and full['ignored']==2 and sum(counts)=={'macos':7684,'linux':7678,'windows':7094}[host],'actual full-suite counts '+host)
    for r in gates:require(r['exit-code']==(101 if r['gate']=='full' and full['fail'] else 0),'host gate exit '+host+'/'+r['gate'])
    require(results(stem+'targeted.log')==[(9 if host=='windows' else 8,0,0)],'nonempty focused ETW count')
    require(results(stem+'previous-wnf-subscription.log')==[(15 if host=='windows' else 14,0,0)],'subscription regression selection')
    require(results(stem+'previous-wnf-event.log')==[(8 if host=='windows' else 7,0,0)],'Event regression selection')
    require(results(stem+'capi.log')[0]==(168,0,0),'C API tests')
    require(results(stem+'integration.log')[0]==(4 if host=='windows' else 544,0,0),'nonempty registered integration')
    command=next(r['command'] for r in gates if r['gate']=='all-targets')
    require('--no-default-features' in command and '--locked' in command,'empty-feature owning build')
    actual=[]
    if full['fail']:
        actual=read(stem+'full.log').rsplit('\nfailures:\n',1)[1].split('\ntest result:',1)[0].split()
    require(actual==summary['tests']['failures'][host],'exact full failure selection '+host)
for r in data('isolated-gates.json'):
    require(r['exit-code']==0 and '--exact' in r['command'] and results(r['log'])==[(1,0,0)],'nonempty unrelated failure isolate')
for host,count in [('macos',5),('linux',2)]:
    gates=data(host+'-owning-gates.json')
    require(len(gates)==count and all(r['exit-code']==0 for r in gates),'owning products '+host)
require('100% tests passed out of 7' in read('macos-owning-ctest.log'),'owning seven CTest selections')
for name in ['macos-owning-protected-assist-cli.log','macos-owning-protected-assist-mcp.log']:require('0 failures' in read(name),'protected text scan')
for host,name in [('linux','linux-owning-cpp.log'),('windows','assist-native-etw-notification-owned-cpp.log')]:
    text=read(name)
    for target in ['process_tool','process_tool_unavailable','rax_process','rax_abi_drift','rust_archive_link']:
        require(('run /validation/'+target+' /validation/'+target) in text if host=='linux' else target+'.exe' in text,'actual C++ consumer '+host+'/'+target)
archive=data(prefix+'owning-archive-hashes.json');frozen=data('owning-source-hashes.json')
require(archive['core-features']==[] and len(archive['sources'])==61 and len(archive['helpers'])==2,'actual source selection')
require(archive['sources']==frozen['sources'] and archive['helpers']==frozen['helpers'],'frozen source/helper bytes')
for r in archive['sources']:
    path=source_root/r['path'];require(path.is_file() and path.stat().st_size==r['bytes'] and sha(path)==r['sha256'],'current compiled source '+r['path'])
require(all(r['structural-member-walk']=='pass' and r['members']>0 for r in archive['archives']),'complete owning archive walk')
require(archive['archives']==summary['owning-archives'],'actual archive summary identity')
applied=data(prefix+'current-source-check.json');overlay=data(prefix+'current-overlay-manifest.json')
require(applied['errors']==[] and applied['source-files']==len(overlay['source-files']) and applied['patches']==len(overlay['patches']),'applied native source snapshot')
require(applied['root-head']==data('task-baseline.json')['root'] and applied['rax-parent']==data('task-baseline.json')['rax'],'native owning baseline')
require(read('root-owning-source-delta.txt').splitlines()==['VENDORED_VERSIONS.md','src/emulation/native-process-emulation.md'],'Unix compiled-code parity')
require(sha(base/(prefix+'after-trace.rs'))=='d41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e','unchanged production observer')
require('terminal turn=50254 Complete(Internal(' in read('before-production-trace.log') and 'NtTraceControl' in read('before-production-trace.log'),'previous unimplemented service baseline')
trace=read(prefix+'after-production-trace-output.log');frontier=summary['frontier']
require(frontier['before-turn']==50254 and frontier['delta-scheduler-calls']==frontier['after-turn']-50254,'scheduler arithmetic')
require(f"terminal turn={frontier['after-turn']} Complete(Internal(" in trace and frontier['service'] in trace,'actual new unsupported frontier')
ordinary=read(prefix+'after-ordinary-processes.log')
require(ordinary.count('program ')==4 and ordinary.count('run reason=5 exit=0 diagnostic=')==4 and ordinary.count(frontier['service'])==4,'all unchanged ordinary failures')
require(frontier['guest-exit']=='not reached','ordinary exit limitation preserved')
print('pass: 15 native producers; 23160 cases; 51 host gates; owning source/archive identities; bounded next frontier')
