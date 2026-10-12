from pathlib import Path
import subprocess,json
base=Path(__file__).parent
results=[]
for name,command in json.loads((base/'macos-owning-commands.json').read_text()):
 with (base/('macos-owning-'+name+'.log')).open('w') as log:r=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT)
 results.append({'gate':name,'command':command,'exit-code':r.returncode});(base/'macos-owning-gates.json').write_text(json.dumps(results,indent=2)+'\n');print(name,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
