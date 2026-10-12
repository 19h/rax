from pathlib import Path
import json,subprocess
base=Path(__file__).parent
root='/Users/int/hexrays/kvasir'
import sys,os
host=sys.argv[1]
if host=='macos': os.environ['CARGO_TARGET_DIR']='/Users/int/Library/Caches/assist-process-emulation/io-completion-create-20261011/red-target'
steps=[('io-completion',['test','--lib','io_completion']),('events',['test','--lib','native_event']),('full',['test','--lib']),('capi',['test','-p','rax-capi']),('all-targets',['build','--all-targets']),('integration',['test','--test','user_windows','--test','user_windows_memory'])]
steps.insert(0,('worker-factory',['test','--lib','worker_factory_create']))
steps.insert(0,('timer2',['test','--lib','timer2_create']))
steps.insert(0,('threading',['test','--lib','dll::threading::tests']))
steps.insert(0,('wait-packet-create',['test','--lib','wait_packet_create']))
steps.insert(0,('wait-packet-associate',['test','--lib','wait_packet_associate']))
steps.insert(0,('targeted',['test','--lib','worker_factory_set_maximum']))
results=[]
for name,args in steps:
    command=['cargo',args[0],'--locked','--offline','--no-default-features']+args[1:]+([] if name=='all-targets' else ['--','--nocapture']+(['--test-threads=1'] if name=='capi' or (host=='linux' and name=='integration') else []))
    if host=='linux':
        command=['docker','run','--rm','--platform','linux/amd64','-v',root+':/work:ro','-v','rax-cargo-registry:/usr/local/cargo/registry','-v','rax-cargo-git:/usr/local/cargo/git','-v','rax-user-target:/target','-e','CARGO_TARGET_DIR=/target','-e','RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu','-w','/work/vendor/rax','rust:1.95-bullseye']+command
    with (base/(host+'-'+name+'.log')).open('w') as log:
        result=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT,cwd=root+'/vendor/rax')
    results.append({'gate':name,'exit-code':result.returncode,'command':command})
    (base/(host+'-gates.json')).write_text(json.dumps(results,indent=2)+'\n')
    print(name,result.returncode,flush=True)
    if result.returncode and name!='full':raise SystemExit(result.returncode)
