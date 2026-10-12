"""Verify bounded native function30 semantics, complete partitions and build provenance."""
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
prefix='native-etw-provider-traits-'
manifest=data('evidence-manifest.json')
require(set(manifest)=={p.name for p in base.iterdir() if p.is_file() and p.name!='evidence-manifest.json'},'exact evidence coverage')
for n,r in manifest.items():require((base/n).stat().st_size==r['bytes'] and sha(base/n)==r['sha256'],'evidence identity '+n)
profiles=['arm64','x64','x86','x86-laa'];families=['matrix','boundaries','format','reference','alias','length-high']
identity=data(prefix+'producer-identities.json')
require(identity['windows-version']=='10.0.29683' and identity['host-machine']=='ARM64','installed host identity')
expected={f'{f}/{p}' for f in families for p in profiles}|{'fresh/arm64','fresh-payload/arm64'}
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
INVALID,AV,GUARD,BUSY,BAD_HANDLE,BAD_TYPE,CORRUPT=0xC000000D,0xC0000005,0x80000001,0xC0000001,0xC0000008,0xC0000024,0xC0000102
POISON=0xA5A5A5A5
def valid(blob):
    if len(blob)<3 or int.from_bytes(blob[:2],'little')!=len(blob):return False
    try:at=blob.index(0,2)+1
    except ValueError:return False
    while at<len(blob):
        if len(blob)-at<3:return False
        n=int.from_bytes(blob[at:at+2],'little')
        if n<3 or n>len(blob)-at:return False
        at+=n
    return True
def simple_blob(length,embedded,name):
    b=bytearray(65536);b[:2]=embedded.to_bytes(2,'little')
    if name==2:b[2:]=b'A'*65534
    elif name in [1,3]:b[2:7]=b'name\0'
    if name==3:b[7:10]=b'\x03\0\x63'
    return b[:length&65535]
def lifetime(r,label):require(r['handles-before']==r['handles-after'] and r['close']=='00000000',label+' close/drain')
def first_retry(r,status,retry,label):
    require(num(r,'status')==status and num(r,'returned')==(0 if status==0 else POISON),label+' status/ReturnLength')
    require(num(r,'retry')==retry and num(r,'retry-returned')==(0 if retry==0 else POISON),label+' retry/once-only')
matrix=[(0,il,ol,0,7,7,1,0,0) for il in [0,1,4,16,23,24,25,32,48,160] for ol in [0,1,4,119,120,121,160,256]]
matrix += [(1,24,ol,m,7,7,1,0,0) for m in [1,2,3,4] for ol in [0,119,120,160]]
matrix += [(2,24,120,0,l,e,n,0,0) for l in [0,1,2,3,4,5,7,16,31,32,33,255,256,257,65534,65535,65536] for e in [0,1,2,3,4,5,7,16,32,255,256,257,65535] for n in range(4)]
matrix += [(3,24,120,0,l,l,0,h,0x100000000 if h==5 else 0) for h in range(7) for l in [0,1,2,3,7,65535,65536]]
matrix += [(4,24,120,0,7,7,1,0,h) for h in [1,0x13572468,0xFFFFFFFF]]
high=[(5,24,120,0,l,e,n,0,0) for l in [65536,65537,65538,65539,65543,131079,0x12340007,0xFFFF0003,0xFFFFFFF9,0xFFFFFFFF] for e in [0,1,2,3,7,65529,65535] for n in [0,1,2]]
formats=[(0,l,n,0,0,0,0) for l in [3,4,7,16,32,255,256,257,65535] for n in range(4)]
formats += [(1,3+rec+d,0,rec,t,d,tail) for rec in [0,1,2,3,4,18,19,20,64,65532] for t in [0,1,2,3,99,255] for d in [-1,0,1,3] for tail in [0,1,2] if 3<=3+rec+d<=65535]
formats += [(2,7+rec,1,rec,t,0,0) for rec in [3,19,20] for t in [0,1,99]]
for profile in profiles:
    for family,expected in [('matrix',matrix),('length-high',high)]:
        n=prefix+family+'-'+profile+'.log';rs=rows(n)
        partition(rs,[(k,16 if k=='high' else 10) for k in ['family','input-length','output-length','mode','length','embedded','name','handle-mode','high']],expected,'exact mature partitions '+family)
        require(f'complete cases={len(expected)}' in read(n),'complete '+family)
        for r in rs:
            il,ol,m,l,e,nm,hm=(num(r,k,10) for k in ['input-length','output-length','mode','length','embedded','name','handle-mode'])
            if il!=24 or ol<120 or m in [1,2,4] or l&65535==0:status=INVALID
            elif hm in [1,2,3,5,6]:status=BAD_HANDLE
            elif hm==4:status=BAD_TYPE
            else:status=0 if valid(simple_blob(l,e,nm)) else CORRUPT
            require(num(r,'status')==status,'traits status')
            require(num(r,'returned')==(0 if status==0 else POISON) and num(r,'repeat')==(BUSY if status==0 else status) and num(r,'repeat-returned')==(0 if status==0 else POISON),'traits once-only/ReturnLength')
            require(num(r,'width',10)==(4 if profile.startswith('x86') else 8),'native caller width')
            require(r['input-before']==r['input-after'] and bytes.fromhex(r['output'])==b'\xA5'*160 and num(r,'canary')==0xB6B6B6B6,'no traits output/input/canary mutation')
            require(r['handles-before']==r['handles-after'] and r['close']==('DEADBEEF' if hm==3 else '00000000'),'traits handle drain')
            b=bytes.fromhex(r['input-before']);require(len(b)==32 and int.from_bytes(b[16:20],'little')==l and int.from_bytes(b[20:24],'little')==(num(r,'high')&0xFFFFFFFF),'captured private header fields')
    n=prefix+'boundaries-'+profile+'.log';rs=rows(n)
    expected=list(itertools.product(range(10),range(7),range(5),[0,23,24,25],[0,119,120,0xFFFFFFFF]))
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
        elif il>12 and ik==6:status=AV
        elif ik==1 or il!=24 or ok==0 or ol<120:status=INVALID
        else:status=0
        require(num(r,'status')==status,'destination/input probe order')
        require(tuple(num(r,k) for k in ['input-protect','returned-protect','returned-upper-protect','output-protect'])==(ip,rp,up,op),'first reached guard consumption')
        require(num(r,'return-after')==(0 if status==0 else POISON) and num(r,'output-after')==POISON,'protection output/ReturnLength')
        retry=BUSY if status==0 else 0
        require(num(r,'retry')==retry and num(r,'retry-returned')==(POISON if retry else 0),'fault-before-publication retry')
        lifetime(r,'base boundaries')
    n=prefix+'format-'+profile+'.log';rs=rows(n)
    partition(rs,[(k,10) for k in ['family','length','name','record','type','delta','tail']],formats,'exact format partitions')
    require('complete cases=711' in read(n),'complete format run')
    for r in rs:
        f,l,nm,rec,t,d,tail=(num(r,k,10) for k in ['family','length','name','record','type','delta','tail'])
        blob=bytearray(65536);blob[:2]=l.to_bytes(2,'little')
        if f==0:
            if nm in [1,2]:blob[2:l]=bytes([0x41 if nm==1 else 0xFF])*(l-2);blob[l-1]=0
            elif nm==3:blob[2:l]=b'A'*(l-2)
        elif f==1:
            blob[3:5]=rec.to_bytes(2,'little');blob[5]=t
            if rec>=19 and t==1 and tail:blob[6:22]=bytes(range(1,17))
            if tail==2 and d>=3:blob[3+rec:6+rec]=b'\x03\0\x63'
        else:
            blob[2:7]=b'name\0';blob[7:9]=rec.to_bytes(2,'little');blob[9]=t
            if rec>=19 and t==1:blob[10:26]=bytes(range(1,17))
        blob=blob[:l];h=14695981039346656037
        for b in blob:h=((h^b)*1099511628211)&0xFFFFFFFFFFFFFFFF
        require(num(r,'payload-hash')==h and bytes.fromhex(r['prefix'])==blob[:64] and bytes.fromhex(r['suffix'])==blob[-64:],'exact format payload identity')
        status=0 if valid(blob) else CORRUPT
        first_retry(r,status,BUSY if status==0 else 0,'structural traits format');lifetime(r,'format')
    n=prefix+'reference-'+profile+'.log';rs=rows(n)
    expected=list(itertools.product(range(2),range(5),range(11),[0,1,2,3,7,4096,65535,65536,0xFFFFFFFF]))
    partition(rs,[(k,10) for k in ['busy','handle-mode','traits','length']],expected,'exact referenced partitions')
    require('complete cases=990' in read(n),'complete referenced run')
    for r in rs:
        busy,hm,tk,l=(num(r,k,10) for k in ['busy','handle-mode','traits','length']);length=l&65535
        if length==0 or tk==1:status=INVALID
        elif hm in [1,3,4]:status=BAD_HANDLE
        elif hm==2:status=BAD_TYPE
        elif busy:status=BUSY
        elif tk in [2,4,10] or tk==6 and length>2:status=AV
        elif tk==3:status=GUARD
        else:status=0 if length==3 else CORRUPT
        retry=BAD_HANDLE if hm==3 else BUSY if busy or status==0 else 0
        first_retry(r,status,retry,'referenced pointer/type/once-only')
        error={0:0,INVALID:87,BAD_HANDLE:6,BAD_TYPE:6,BUSY:31,AV:998,GUARD:GUARD,CORRUPT:1392}[status]
        require(num(r,'error',10)==error,'actual RTL status translation')
        protect=4 if tk==3 and status==GUARD else 0x104 if tk==3 else 1 if tk==4 else 2 if tk==5 else 4
        require(num(r,'protect')==protect,'referenced guard/type/busy precedence')
        require(r['handles-before']==r['handles-after'] and r['close']==('DEADBEEF' if hm==3 else '00000000'),'referenced resource drain')
    n=prefix+'alias-'+profile+'.log';rs=rows(n)
    expected=list(itertools.product(range(2),[-4,-1,0,1,2,3,7,8,15,16,20,23,24,119,120,256],[-80,-3,-2,-1,0,1,2,3,80],[3,7,32]))
    partition(rs,[(k,10) for k in ['returned-base','returned-delta','output-delta','length']],expected,'exact alias partitions')
    require('complete cases=864' in read(n),'complete alias run')
    for r in rs:
        first_retry(r,0,BUSY,'alias');lifetime(r,'alias')
        b=bytearray.fromhex(r['before']);require(len(b)==576,'alias window width')
        at=(288 if num(r,'returned-base',10)==0 else 32)+num(r,'returned-delta',10)
        b[at:at+4]=bytes(4)
        require(bytes(b)==bytes.fromhex(r['after']),'only ReturnLength store after capture')
for family in ['fresh','fresh-payload']:
    n=prefix+family+'-arm64.log';rs=rows(n)
    require([num(r,'case',10) for r in rs]==[0,1,2] and [r['warmup'] for r in rs]==['1','0','0'] and 'complete cases=2 warmup=1' in read(n),'fresh complete cases/excluded warmup')
    for r in rs:
        c=r['case'];entry=[x for x in rows(n,'entry ') if x['case']==c];ret=[x for x in rows(n,'return ') if x['case']==c];images=[x for x in rows(n,'image ') if x['case']==c]
        require(len(images)==1 and num(images[0],'type')==0x1000000 and len(bytes.fromhex(images[0]['bytes']))==32,'installed MEM_IMAGE entry')
        require([num(x,'function',10) for x in entry]==[27,15,30] and len(ret)==3,'fresh actual entry sequence')
        for e,o in zip(entry,ret):require(num(e,'pc')==num(images[0],'entry')+4 and num(o,'pc')==num(e,'lr')+4 and e['thread']==o['thread'] and o['status']=='00000000','true original LR return')
        bs={x['phase']:bytes.fromhex(x['data']) for x in rows(n,'bytes ') if x['case']==c}
        require(len(bs['input-before'])==24 and bs['input-before']==bs['input-after'] and int.from_bytes(bs['input-before'][16:18],'little')==32,'fresh private header/copy')
        require(bs['output-before']==bs['output-after']==bytes(120) and bs['returned-after']==bytes(4),'fresh no output and ReturnLength zero')
        if family=='fresh-payload':require(bs['traits-before']==bs['traits-after']==b'\x20\0EtwUserModeDiagnosticProvider\0','actual fresh installed provider payload')
        require(r['installed']==r['done']=='1' and r['exit']=='77' and r['calls']=='3','owned debugger child completion')
        if c!='0':require(r['handles-before']==r['handles-after'] and r['exact-handle-set']=='1','fresh drained handle set')
summary=data('validation-summary.json')
require(tuple(summary[k] for k in ['native-complete-cases','native-excluded-debugger-warmups','native-verified-producers','native-replayed-core-rows'])==(37632,2,26,37628),'case accounting')
required={'targeted','previous-etw-registration','previous-etw-notification','previous-wnf-subscription','previous-wnf-event','binding','soft-maximum','hard-maximum','wait-packet-associate','wait-packet-create','threading','timer2','worker-factory','io-completion','events','full','capi','all-targets','integration'}
for host,counts in [('macos', (7699, 0, 2, 0)), ('linux', (7693, 0, 2, 0)), ('windows', (7106, 5, 2, 0))]:
    stem=prefix if host=='windows' else host+'-';gates=data(stem+'gates.json')
    require(len(gates)==19 and {r['gate'] for r in gates}==required,'nineteen host gates '+host)
    require(results(stem+'full.log')==[counts] and summary['core'][host]==[list(counts)],'nonfiltered full core suite '+host)
    for r in gates:require(r['exit-code']==(101 if r['gate']=='full' and counts[1] else 0),'host gate exit '+host+'/'+r['gate'])
    require(results(stem+'targeted.log')==[tuple(summary['focused'][host][0])] and summary['focused'][host][0][0]==(9 if host=='windows' else 8),'focused replay count')
    require(results(stem+'capi.log')[0]==(168,0,0,0) and '--test-threads=1' in str(next(r['command'] for r in gates if r['gate']=='capi')),'serial complete C API')
    require(results(stem+'integration.log')[0]==(4 if host=='windows' else 544,0,0,0),'nonempty integrations')
    require('--no-default-features' in str(next(r['command'] for r in gates if r['gate']=='all-targets')),'empty feature all targets')

for host,focused,previous in [('macos',(8,0,0,7693),(17,0,0,7684)),('linux',(8,0,0,7687),(17,0,0,7678)),('windows',(9,0,0,7104),(19,0,0,7094))]:
    stem=prefix if host=='windows' else host+'-'
    require(results(stem+'targeted.log')==[focused] and results(stem+'previous-etw-registration.log')==[previous],'exact traits and registration nonempty test selection '+host)
known_failures={
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry',
 'smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards',
 'smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime'}
require(set(re.findall(r'^test (\S+) \.\.\. FAILED$',read(prefix+'full.log'),re.M))==known_failures,'exact untouched Windows full-suite failures')
for host in ['macos','linux']:
    for kind in ['portable','owning']:require(data(host+'-'+kind+'-exit.json')['exit-code']==0,'Unix driver terminal exit '+host+'/'+kind)
for host,count in [('macos',5),('linux',2)]:require(len(data(host+'-owning-gates.json'))==count and all(r['exit-code']==0 for r in data(host+'-owning-gates.json')),'owning products '+host)
require('100% tests passed out of 7' in read('macos-owning-ctest.log'),'seven owning CTests')
for n in ['macos-owning-protected-assist-cli.log','macos-owning-protected-assist-mcp.log']:require('0 failures' in read(n),'protected text scans')
for host,n in [('linux','linux-owning-cpp.log'),('windows','assist-native-etw-provider-traits-owned-cpp.log')]:
    text=read(n)
    for target in ['process_tool','process_tool_unavailable','rax_process','rax_abi_drift','rust_archive_link']:require(('run /validation/'+target+' /validation/'+target) in text if host=='linux' else target+'.exe' in text,'C++ consumer '+host+'/'+target)
require(data('windows-owning-exit.json')['exit-code']==data('windows-gates-exit.json')['exit-code']==0,'native driver terminal exit')
archive=data(prefix+'owning-archive-hashes.json');frozen=data('owning-source-hashes.json')
require(archive['core-features']==[] and len(archive['sources'])==68 and len(archive['helpers'])==2 and archive['sources']==frozen['sources'] and archive['helpers']==frozen['helpers'],'actual compiled source and helpers')
for r in archive['sources']:
    p=source_root/r['path'];require(p.is_file() and p.stat().st_size==r['bytes'] and sha(p)==r['sha256'],'current compiled source '+r['path'])
for r in archive['helpers']:require((base/r['path']).stat().st_size==r['bytes'] and sha(base/r['path'])==r['sha256'],'compiled helper '+r['path'])
require([(r['bytes'],r['members'],r['sha256'],r['structural-member-walk']) for r in archive['archives']]==[(1118463976, 4673, 'e4695e1c1b45c46952f7c90d2dd9a768fe9506c18718a26a837539f203348afc', 'pass'), (731186588, 260, '7112d16455a92c56ec074c0f027d929fa9d82c5146852a75df66f1e746353790', 'pass')],'actual archive identities and complete walks')
applied=data(prefix+'current-source-check.json');overlay=data(prefix+'current-overlay-manifest.json');baseline=data('task-baseline.json')
require(applied['errors']==[] and applied['source-files']==len(overlay['source-files'])==17883 and applied['patches']==len(overlay['patches'])==218,'complete native overlay')
require(applied['root-head']==baseline['root'] and applied['rax-parent']==baseline['rax'] and applied['environment-adapters']==overlay['environment-adapters'],'native root/source baseline')
require(len(read('root-owning-source-delta.txt').splitlines())==11 and 'src/core/masking_python_runtime.cpp' in read('root-owning-source-delta.txt'),'explicit frozen Unix root limitation')
require(sha(base/(prefix+'after-trace.rs'))==sha(base/'before-ordinary-trace.rs')=='d41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e','unchanged ordinary observer')
require('terminal turn=50692 Complete(Internal(' in read('before-production-trace.log') and 'NtTraceControl function 30' in read('before-production-trace.log'),'actual previous frontier')
f=summary['frontier'];trace=read(prefix+'after-production-trace-output.log');ordinary=read(prefix+'after-ordinary-processes.log')
require(f['before-turn']==50692 and f['after-turn']==90924 and f['delta-scheduler-calls']==40232,'scheduler frontier arithmetic')
require('terminal turn=90924 ' in trace and f['terminal']==trace.split('terminal turn=')[-1].strip() and f['ordinary-result']==ordinary.strip() and ordinary.count('program ')==4,'actual unchanged ordinary observer and programs')
require(f['service']=='NtOpenDirectoryObject' and f['service-number']=='0x58' and 'NtOpenDirectoryObject (0x58)' in trace and ordinary.count('run reason=5 exit=0 diagnostic=')==ordinary.count('NtOpenDirectoryObject (0x58)')==4,'next actual service before guest exit')
require(len(data('assumption-register.json'))==3 and data('bounded-scope.json')['high'][0]['blocks-full-goal'],'bounded assumptions and complete-goal limitation')
print('pass: 26 native producers; 37632 complete cases; 37628 directly replayed rows; 57 host gates; current sources/archives; measured ordinary frontier')
