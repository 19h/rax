from pathlib import Path
import json,subprocess
base=Path(__file__).parent
rax='/Users/int/hexrays/kvasir/vendor/rax'
mounts=['docker','run','--rm','--platform','linux/amd64','-v',str(base/'assist-root-current')+':/work:ro','-v',rax+':'+rax+':ro','-v',str(base)+':/validation','-v','rax-cargo-registry:/usr/local/cargo/registry','-v','rax-cargo-git:/usr/local/cargo/git','-v','assist-native-linux-root-target:/target','-e','CARGO_TARGET_DIR=/target','-e','RUSTUP_TOOLCHAIN=1.95.0-x86_64-unknown-linux-gnu']
steps=[('shipping',mounts+['-w','/work/rust/assist-rs','rust:1.95-bullseye','cargo','build','--locked','--offline','--features','rax']),('cpp',mounts+['-v',str(base/'macos-owning-build/generated/static-text')+':/generated:ro','-v',str(base/'macos-owning-build/_deps/monocypher-src/src')+':/monocypher:ro','-v',str(base/'macos-owning-build/_deps/json-src/single_include')+':/json:ro','rust:1.95-bullseye','python3','/validation/owning-linux-cpp-build.py'])]
results=[]
for name,args in steps:
    with (base/('linux-owning-'+name+'.log')).open('w') as f:result=subprocess.run(args,stdout=f,stderr=subprocess.STDOUT)
    results.append({'gate':name,'exit-code':result.returncode,'command':args});(base/'linux-owning-gates.json').write_text(json.dumps(results,indent=2)+'\n')
    print(name,result.returncode,flush=True)
    if result.returncode:raise SystemExit(result.returncode)
