from pathlib import Path
import datetime
import hashlib
import json
import re
import subprocess

M = Path(__file__).resolve().parent
MAIN = Path('/Users/int/hexrays/kvasir')
R = M / 'assist-root-mutant-final'
PARENT = '390309d0a09f58643c38e7014adea82742bd2c41'

def run(args, cwd=MAIN):
    return subprocess.check_output(args,cwd=cwd,text=True).strip()

status = run(['git','status','--short','--branch'])
heads = {remote:run(['git','ls-remote',remote,'refs/heads/master']).split()[0] for remote in ['origin','19h']}
for remote in heads:
    run(['git','fetch','--no-recurse-submodules',remote,'master'])
head = heads['origin']
if len(set(heads.values())) != 1:
    if subprocess.run(['git','merge-base','--is-ancestor',heads['19h'],head],cwd=MAIN).returncode == 0:
        pass
    elif subprocess.run(['git','merge-base','--is-ancestor',head,heads['19h']],cwd=MAIN).returncode == 0:
        head = heads['19h']
    else:
        raise RuntimeError('root remotes diverged; inspect before replaying owned records')
if run(['git','rev-parse',head+':vendor/rax']) != PARENT:
    raise RuntimeError('upstream RAX pin changed')
compared = json.loads((M/'current-cpp-consumers-mutant-root.json').read_text())
rows=[]
for old in compared['files']:
    raw=subprocess.check_output(['git','show',head+':'+old['path']],cwd=MAIN)
    new={'path':old['path'],'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}
    equal=old==new
    if not equal and (old['path']!='src/crypto/static_text.cpp' or new['sha256']!='1835211daeff4b7089081214c070f78f73606ec0f9a1f99cf448280402580372'):
        raise RuntimeError('unreviewed affected C++ source drift '+old['path'])
    rows.append({'compared':old,'upstream':new,'equal':equal})
if sum(r['equal'] for r in rows)!=8:
    raise RuntimeError('unexpected current-source comparison')
header=run(['git','show',head+':rust/assist-rs/include/assist_rs.h'])
minor=int(re.search(r'#define ASSIST_RS_ABI_MINOR (\d+)u',header)[1])
if not R.exists():
    run(['git','worktree','add','--detach',str(R),head])
else:
    if run(['git','status','--porcelain'],R):raise RuntimeError('root publication tree is dirty')
    run(['git','merge','--ff-only',head],R)
record={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'remote-heads-before':heads,'upstream-parent':head,
        'upstream-abi-minor':minor,'compared-root':compared['current-root-head'],
        'compared-abi-minor':compared['current-abi-minor'],'frozen-carrier-abi-minor':184,
        'files':rows,'matched-affected-files':8,'main-status-before':status,
        'scope':'Eight RAX adapter/consumer source files match. The separately reviewed static-text migration is unchanged from the BNO review. Frozen ABI184 carrier results remain distinct from whole-current-root products.'}
(M/'mutant-upstream-source-comparison.json').write_text(json.dumps(record,indent=2)+'\n')
(M/'mutant-root-base.json').write_text(json.dumps({'base':head,'upstream-abi-minor':minor},indent=2)+'\n')
print('Prepared root snapshot',head,'ABI',minor,'with eight unchanged RAX/consumer sources')
