from pathlib import Path
import sys,subprocess,json,time,hashlib,os
c=Path(__file__).parent;host=sys.argv[1];rows=[]
for row in json.loads((c/(host+'-image-owning-commands.json')).read_text()):
 log=c/(host+'-image-owning-'+row['gate']+'.log');start=time.time()
 with log.open('wb') as out:code=subprocess.run(row['command'],cwd=row.get('cwd'),stdout=out,stderr=subprocess.STDOUT,env=dict(os.environ,RUSTUP_TOOLCHAIN='1.95.0')).returncode
 row.update({'exit-code':code,'seconds':time.time()-start,'log':log.name,'sha256':hashlib.sha256(log.read_bytes()).hexdigest()});rows.append(row);(c/(host+'-image-owning-gates.json')).write_text(json.dumps(rows,indent=2)+'\n');print(host,'owning',row['gate'],'exit',code,flush=True)
 if code:raise SystemExit(code)
