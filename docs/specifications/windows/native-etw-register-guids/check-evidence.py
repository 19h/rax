"""Verify bounded native function15 semantics, complete partitions and build provenance."""
from pathlib import Path
import hashlib,itertools,json,re,sys

def require(ok,message):
    if not ok:raise ValueError(message)
require(not sys.flags.optimize,'optimized validation rejected')
base=Path(sys.argv[1]) if len(sys.argv)>1 else Path(__file__).parent
source_root=Path(sys.argv[2]) if len(sys.argv)>2 else Path(__file__).resolve().parents[4]
def read(n):return (base/n).read_text(encoding='utf-8-sig')
def data(n):return json.loads(read(n))
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def rows(n,prefix='case='):return [dict(f.split('=',1) for f in l.split() if '=' in f) for l in read(n).splitlines() if l.startswith(prefix)]
def num(r,k,radix=16):return int(r[k],radix)
def results(n):return [tuple(map(int,x)) for x in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;[^\n]*?(\d+) filtered out;',read(n))]
def partition(observed,keys,expected,label):
    require([num(r,'case',10) for r in observed]==list(range(len(expected))) and [tuple(num(r,k,radix) for k,radix in keys) for r in observed]==expected,label)
def reply(request,h):
    b=bytearray(request[:160]);b[24:32]=h.to_bytes(8,'little');b[44:48]=(160).to_bytes(4,'little');b[112:160]=bytes(48);return b
prefix='native-etw-register-guids-'
manifest=data('evidence-manifest.json')
require(set(manifest)=={p.name for p in base.iterdir() if p.is_file() and p.name!='evidence-manifest.json'},'exact evidence coverage')
for n,r in manifest.items():require((base/n).stat().st_size==r['bytes'] and sha(base/n)==r['sha256'],'evidence identity '+n)
profiles=['arm64','x64','x86','x86-laa'];families=['matrix','boundaries','alias-duplicate','capacity','duplicate','duplicate-buffers']
identity=data(prefix+'producer-identities.json')
require(identity['windows-version']=='10.0.29683' and identity['host-machine']=='ARM64','installed host identity')
expected={f'{f}/{p}' for f in families for p in profiles}|{'fresh/arm64','without-event/arm64'}
require(len(identity['producers'])==26 and {r['profile'] for r in identity['producers']}==expected,'exact native producers')
for r in identity['producers']:
    for key,name in [('source_identity',r['source']),('log_identity',r['log'])]:
        d=r[key];require(d['bytes']==(base/name).stat().st_size and d['sha256']==sha(base/name),'actual producer '+key)
    require(r['source-sha256']==r['source_identity']['sha256'],'producer source binding')
    profile=r['profile'].split('/')[1];e=r['executable_identity']
    require(e['machine']=={'arm64':0xAA64,'x64':0x8664,'x86':0x14C,'x86-laa':0x14C}[profile],'actual PE architecture')
    require(e['bytes']>0 and re.fullmatch('[a-f0-9]{64}',e['sha256']),'actual PE identity')
    require(bool(e['characteristics']&0x20) if profile=='x86-laa' else True,'PE LAA flag')
require([r['sha256'] for r in identity['native-libraries']]==['502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f','ffd2b7ff39889c5599c8a8451eef5c852acee9101da5ce5a6def8b8e60a6627e'],'pinned installed NTDLL identities')
require(sha(base/'phnt-ntwmi.h')=='ea487f6366b9678ab31a2f8509b4fbc0be63d4deb83a86b123da83c66b21cf21','pinned PHNT source')
INVALID,AV,GUARD=0xC000000D,0xC0000005,0x80000001
POISON=0xA5A5A5A5
mature=list(itertools.product([0],[0],[0],[0,1,4,16,24,32,48,159,160,161,168,192,320],[0,1,4,159,160,161,192,320],[0]))
mature +=[(1,16,v,160,160,0) for v in [0,1,2,3,4,5,6,7,8,16,32,65535,0xFFFFFFFF]]
mature +=[(1,20,v,160,160,0) for v in [0,1,2,3,4,16,65536,0xFFFFFFFF]]
mature +=[(2,o,v,160,160,0) for o in [24,32] for v in [0,1,0x13572468,0xFFFFFFFFFFFFFFFF]]
mature +=[(1,o,0xFFFFFFFF,160,160,0) for o in range(40,160,4)]
mature +=[(3,o,255,160,160,0) for o in range(0,16,4)]+[(4,0,0,160,160,0)]
mature +=[(0,0,0,160,ol,m) for m in [1,2,3,4] for ol in [0,159,160,192]]
for profile in profiles:
    n=prefix+'matrix-'+profile+'.log';rs=rows(n)
    partition(rs,[(k,16 if k=='value' else 10) for k in ['family','offset','value','input-length','output-length','mode']],mature,'exact mature partitions')
    require('complete cases=184' in read(n),'complete mature run')
    for r in rs:
        il,ol,mode=(num(r,k,10) for k in ['input-length','output-length','mode']);status=0 if il==160 and ol>=160 and mode in [0,3] else INVALID
        require(num(r,'status')==status,'registration status')
        require(r['handles-before']==r['handles-after'] and num(r,'canary')==0xB6B6B6B6,'mature resource and canary')
        before=bytes.fromhex(r['input-before']);after=bytes.fromhex(r['input-after']);out=bytes.fromhex(r['output'])
        if status==0:
            h=num(r,'handle');expected=reply(before,h)+before[160:]
            require(num(r,'returned')==160 and h!=0,'registration reply length/handle')
            require(after==expected and out==b'\xA5'*192 if mode==3 else after==before and out==reply(before,h)+b'\xA5'*32,'captured request and reply copy')
            require(r['type']=='EtwRegistration' and num(r,'grant')==0x804 and r['query']==r['type-query']==r['close']=='00000000','typed registration resource')
            require(num(r,'basic-bytes',10)==56 and num(r,'type-bytes',10)==(128 if profile.startswith('x86') else 136) and num(r,'wait')==0xC0000022 and num(r,'duplicate')==50,'registration query/wait/duplicate')
        else:require(num(r,'returned')==POISON and before==after and out==b'\xA5'*192,'no failure publication')
    n=prefix+'boundaries-'+profile+'.log';rs=rows(n)
    expected=list(itertools.product(range(10),range(7),range(5),[0,159,160,161],[0,159,160,0xFFFFFFFF]))
    partition(rs,[(k,10) for k in ['input','returned','output','input-length','output-length']],expected,'exact protection partitions')
    require('complete cases=5600' in read(n),'complete protection run')
    for r in rs:
        ik,rk,ok,il,ol=(num(r,k,10) for k in ['input','returned','output','input-length','output-length'])
        ip=0x104 if ik==3 else 1 if ik==4 else 2 if ik==5 else 4
        rp=0x104 if rk==3 else 1 if rk==4 else 2 if rk==5 else 4
        up=1 if rk==6 else 4;op=1 if ok==2 else 0x104 if ok==3 else 2 if ok==4 else 4
        if ol and ok>=2:
            status=GUARD if ok==3 else AV
            if ok==3:op=4
        elif ol==0xFFFFFFFF and ok==1:status=AV
        elif rk==1:status=INVALID
        elif rk in [2,4,5,6]:status=AV
        elif rk==3:status=GUARD;rp=4
        elif il and ik in [2,4]:status=AV
        elif il and ik==3:status=GUARD;ip=4
        elif il>80 and ik==6:status=AV
        elif ik==1 or il!=160 or ok==0 or ol<160:status=INVALID
        else:status=0
        require(num(r,'status')==status,'input read probe before class length')
        require(tuple(num(r,k) for k in ['input-protect','returned-protect','returned-upper-protect','output-protect'])==(ip,rp,up,op),'first reached guard consumption')
        require(num(r,'return-after')==(160 if status==0 else POISON) and num(r,'output-after')==(0xD4BCDC21 if status==0 else POISON),'protection stores')
        require(r['close']==('00000000' if status==0 else 'DEADBEEF'),'protection resource close')
    n=prefix+'alias-duplicate-'+profile+'.log';rs=rows(n,'alias ')
    expected=list(itertools.product([-80,-3,-2,-1,0,1,2,3,80],[-4,0,1,2,3,16,24,28,32,44,48,112,156,158,160,256]))
    partition(rs,[(k,10) for k in ['output-delta','returned-delta']],expected,'exact alias partitions')
    require('alias-complete cases=144' in read(n),'complete alias run')
    for r in rs:
        before=bytes.fromhex(r['before']);expected=bytearray(before);od,rd=(num(r,k,10) for k in ['output-delta','returned-delta']);h=num(r,'actual-handle')
        expected[112+od:272+od]=reply(before[112:272],h);expected[112+rd:116+rd]=(160).to_bytes(4,'little')
        require(bytes(expected)==bytes.fromhex(r['after']) and r['status']==r['close']=='00000000' and num(r,'returned')==160,'alias copy then ReturnLength and actual resource')
    duplicate_results={}
    for family,options in [('duplicate',[0,1,2,3,4,5,6,7,8,16,32,0xFFFFFFFF]),('alias-duplicate',[0,1,2,3,4])]:
        n=prefix+family+'-'+profile+'.log';rs=rows(n,'duplicate ');expected=list(itertools.product(options,[0,4,0x804,0xFFFFFFFF],range(3),range(2)))
        partition(rs,[(k,16 if k=='grant' else 10) for k in ['options','grant','output-mode','target-mode']],expected,'exact duplicate partitions')
        require(f'duplicate-complete cases={len(expected)}' in read(n),'complete duplicate run')
        for r in rs:
            opt,g,om,tm=(num(r,k,16 if k=='grant' else 10) for k in ['options','grant','output-mode','target-mode']);denied=not(opt&2) and bool(g&~0x804)
            error=998 if om==2 else 5 if denied else 50 if tm==0 else 0xB6B6B6B6 if opt&1 else 87
            live=not(bool(opt&1) and not denied and (om!=2 or profile.startswith('x86')))
            require(num(r,'error')==error and num(r,'ok',10)==int(error==0xB6B6B6B6) and num(r,'source-live',10)==live,'duplicate rights/copy/close ordering')
            require(num(r,'output')==(0 if om==0 else POISON) and num(r,'exception')==0 and r['extras']=='0' and r['restored']=='1','duplicate output and resource restoration')
            signature=tuple(r[k] for k in ['ok','error','exception','output','source-live','flags','close','extras','restored'])
            key=(opt,g,om,tm)
            if family=='duplicate':duplicate_results[key]=signature
            else:require(duplicate_results[key]==signature,'repeated native duplicate agreement')
    n=prefix+'duplicate-buffers-'+profile+'.log';rs=rows(n);expected=list(itertools.product(range(5),range(4),range(4),[0,0xFFFFFFFF]))
    partition(rs,[(k,16 if k=='grant' else 10) for k in ['kind','offset','options','grant']],expected,'exact duplicate buffer partitions')
    require('complete cases=160' in read(n),'complete duplicate buffer run')
    for r in rs:
        kind,opt,g=(num(r,k,16 if k=='grant' else 10) for k in ['kind','options','grant']);denied=not(opt&2) and bool(g&~0x804)
        error=(5 if denied else 50) if kind==0 else GUARD if kind==3 else 998
        live=not(bool(opt&1) and not denied and (kind==0 or profile.startswith('x86')))
        expected_out=0 if kind==0 else int.from_bytes(b'\xA5'*num(r,'width',10),'little')
        require(num(r,'error')==error and r['ok']=='0' and r['exception']=='00000000' and num(r,'source-live',10)==live,'duplicate output fault and ABI close behavior')
        require(num(r,'output')==expected_out and num(r,'protect')==(2 if kind==1 else 1 if kind==2 else 4) and r['handles-before']==r['handles-after'],'duplicate guard/canary/drain')
    n=prefix+'capacity-'+profile+'.log';rs=rows(n);partition(rs,[(k,10) for k in ['unique','ordinal']],list(itertools.product(range(2),range(1030))),'exact capacity partitions')
    require('complete cases=2060' in read(n),'complete capacity run')
    for r in rs:require(r['status']=='00000000' and num(r,'returned')==160 and num(r,'canary')==0xB6B6B6B6 and num(r,'live',10)==num(r,'baseline',10)+num(r,'ordinal',10)+1 and num(r,'successful',10)==num(r,'ordinal',10)+1,'1030 live registrations')
    drains=rows(n,'drain ');require(len(drains)==2 and all(r['handles-before']==r['handles-after'] and r['registered']=='1030' for r in drains),'capacity full drain')
for family in ['fresh','without-event']:
    n=prefix+family+'-arm64.log';rs=rows(n);require([num(r,'case',10) for r in rs]==[0,1,2] and [r['warmup'] for r in rs]==['1','0','0'] and 'complete cases=2 warmup=1' in read(n),'fresh cases and excluded warmup')
    for r in rs:
        c=r['case'];entry=[x for x in rows(n,'entry ') if x['case']==c];ret=[x for x in rows(n,'return ') if x['case']==c];images=[x for x in rows(n,'image ') if x['case']==c]
        require(len(images)==1 and num(images[0],'type')==0x1000000 and len(bytes.fromhex(images[0]['bytes']))==32,'installed MEM_IMAGE entry')
        require([num(x,'function',10) for x in entry]==([27,15] if family=='fresh' else [27]) and len(entry)==len(ret),'fresh actual entry sequence')
        for e,o in zip(entry,ret):require(num(e,'pc')==num(images[0],'entry')+4 and num(o,'pc')==num(e,'lr')+4 and e['thread']==o['thread'] and o['status']=='00000000','true original LR return')
        bs={x['phase']:bytes.fromhex(x['data']) for x in rows(n,'bytes ') if x['case']==c};out=bs['output-after'];h=int.from_bytes(out[24:32],'little')
        require(h!=0 and out==reply(bs['input-before'],h) and bs['returned-after']==(160).to_bytes(4,'little'),'fresh native reply')
        if family=='without-event':require(len([x for x in rows(n,'redirect ') if x['case']==c and x['original-function']=='27'])==1 and bs['input-before']==bs['input-after'],'owned diagnostic redirect before Event')
        require(r['installed']==r['done']=='1' and r['exit']=='77' and r['calls']==('2' if family=='fresh' else '1'),'owned child termination')
        if c!='0':require(r['handles-before']==r['handles-after'] and r['exact-handle-set']=='1','fresh drained handle set')
summary=data('validation-summary.json')
require(tuple(summary[k] for k in ['native-complete-cases','native-excluded-debugger-warmups','native-verified-producers','native-replayed-core-rows'])==(34228,2,26,33744),'case accounting')
required={'targeted','previous-etw-notification','previous-wnf-subscription','previous-wnf-event','binding','soft-maximum','hard-maximum','wait-packet-associate','wait-packet-create','threading','timer2','worker-factory','io-completion','events','full','capi','all-targets','integration'}
for host,counts in [('macos',(7691,0,2,0)),('linux',(7684,1,2,0)),('windows',(7097,5,2,0))]:
    stem=prefix if host=='windows' else host+'-';gates=data(stem+'gates.json')
    require(len(gates)==18 and {r['gate'] for r in gates}==required,'eighteen host gates '+host)
    require(results(stem+'full.log')==[counts] and summary['core'][host]==[list(counts)],'nonfiltered full core suite '+host)
    for r in gates:require(r['exit-code']==(101 if r['gate']=='full' and counts[1] else 0),'host gate exit '+host+'/'+r['gate'])
    require(results(stem+'targeted.log')==[tuple(summary['focused'][host][0])] and summary['focused'][host][0][0]==(10 if host=='windows' else 9),'focused replay count')
    require(results(stem+'capi.log')[0]==(168,0,0,0) and '--test-threads=1' in str(next(r['command'] for r in gates if r['gate']=='capi')),'serial complete C API')
    require(results(stem+'integration.log')[0]==(4 if host=='windows' else 544,0,0,0),'nonempty integrations')
    require('--no-default-features' in str(next(r['command'] for r in gates if r['gate']=='all-targets')),'empty feature all targets')
require(results('linux-isolated-a_timeout_is_removed_or_updated.log')==[(1,0,0,7686)] and all(r['exit-code']==0 and '--exact' in r['command'] for r in data('isolated-gates.json')),'exact unchanged timeout isolate')
timeout=data('timeout-unchanged-source.json');require(timeout['unchanged'] and sha(source_root/timeout['path'])==timeout['sha256'],'unchanged timeout source identity')
for host,count in [('macos',5),('linux',2)]:require(len(data(host+'-owning-gates.json'))==count and all(r['exit-code']==0 for r in data(host+'-owning-gates.json')),'owning products '+host)
require('100% tests passed out of 7' in read('macos-owning-ctest.log'),'seven owning CTests')
for n in ['macos-owning-protected-assist-cli.log','macos-owning-protected-assist-mcp.log']:require('0 failures' in read(n),'protected text scans')
for host,n in [('linux','linux-owning-cpp.log'),('windows','assist-native-etw-register-guids-owned-cpp.log')]:
    text=read(n)
    for target in ['process_tool','process_tool_unavailable','rax_process','rax_abi_drift','rust_archive_link']:require(('run /validation/'+target+' /validation/'+target) in text if host=='linux' else target+'.exe' in text,'C++ consumer '+host+'/'+target)
require(data('windows-owning-exit.json')['exit-code']==data('windows-gates-exit.json')['exit-code']==0,'native driver terminal exit')
archive=data(prefix+'owning-archive-hashes.json');frozen=data('owning-source-hashes.json')
require(archive['core-features']==[] and len(archive['sources'])==65 and len(archive['helpers'])==2 and archive['sources']==frozen['sources'] and archive['helpers']==frozen['helpers'],'actual compiled source and helpers')
for r in archive['sources']:
    p=source_root/r['path'];require(p.is_file() and p.stat().st_size==r['bytes'] and sha(p)==r['sha256'],'current compiled source '+r['path'])
for r in archive['helpers']:require((base/r['path']).stat().st_size==r['bytes'] and sha(base/r['path'])==r['sha256'],'compiled helper '+r['path'])
require([(r['bytes'],r['members'],r['sha256'],r['structural-member-walk']) for r in archive['archives']]==[(1122182874,4673,'9f03126d10a581b9f999fb1e5a2851977f72caabba2b16716fa8a17d3473c2aa','pass'),(734900632,260,'9c3e28792df9658e3cd6edcfed712f1435b049d13ee1c9aa87f9522ef71b3c62','pass')],'actual archive identities and complete walks')
applied=data(prefix+'current-source-check.json');overlay=data(prefix+'current-overlay-manifest.json');baseline=data('task-baseline.json')
require(applied['errors']==[] and applied['source-files']==len(overlay['source-files'])==17676 and applied['patches']==len(overlay['patches'])==218,'complete native overlay')
require(applied['root-head']==baseline['root'] and applied['rax-parent']==baseline['rax'] and applied['environment-adapters']==overlay['environment-adapters'],'native root/source baseline')
require(len(read('root-owning-source-delta.txt').splitlines())==11 and 'src/core/masking_python_runtime.cpp' in read('root-owning-source-delta.txt'),'explicit frozen Unix root limitation')
require(sha(base/(prefix+'after-trace.rs'))==sha(base/'before-ordinary-trace.rs')=='d41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e','unchanged ordinary observer')
require('terminal turn=50306 Complete(Internal(' in read('before-production-trace.log') and 'NtTraceControl function 15' in read('before-production-trace.log'),'actual previous frontier')
f=summary['frontier'];trace=read(prefix+'after-production-trace-output.log');ordinary=read(prefix+'after-ordinary-processes.log')
require(f['before-turn']==50306 and f['after-turn']==50692 and f['delta-scheduler-calls']==50692-50306==386 and f['function']==30,'scheduler frontier arithmetic')
require('terminal turn=50692 Complete(Internal(' in trace and 'NtTraceControl function 30' in trace and ordinary.count('program ')==ordinary.count('run reason=5 exit=0 diagnostic=')==ordinary.count('NtTraceControl function 30')==4,'actual unchanged ordinary programs before exit')
require(len(data('assumption-register.json'))==3 and data('bounded-scope.json')['high'][0]['blocks-full-goal'],'bounded assumptions and complete-goal limitation')
print('pass: 26 native producers; 34228 complete cases; 33744 directly replayed rows; 54 host gates; current sources/archives; function30 before guest exit')
