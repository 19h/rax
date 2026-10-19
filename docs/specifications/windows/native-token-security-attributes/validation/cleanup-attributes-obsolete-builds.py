from pathlib import Path
import datetime,hashlib,json,os,shutil,subprocess
K=Path(__file__).parent;C=K.parent;R=Path('/Users/int/.assist-token-20261019');root=C/'wait-completion-packet-create-20261012/assist-root-current';old=K/'attributes-source/target';legacy=C/'wait-completion-packet-create-20261012/macos-owning-build'
for name in ['attributes-macos-epoch2-gates.json','attributes-linux-epoch2-gates.json','macos-attributes-epoch2-owning-gates-c.json','linux-attributes-epoch2-owning-gates-b.json']:
 assert json.loads((K/name).read_text())['state']=='finished',name
processes=subprocess.check_output(['ps','-eo','pid,args'],text=True)
for p in [old,legacy]:
 rows=[line for line in processes.splitlines() if str(p) in line]
 assert not rows,(str(p),rows)
assert not old.is_symlink() and old.is_dir() and (old/'CACHEDIR.TAG').is_file()
# The relocated final Cargo output is a separate CoW tree; generated outputs only.
assert (R/'target/debug').is_dir() and (root/'build-native-attributes/assist-rs-target').is_dir()
record={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'removed':[],'preserved':['All source worktrees and evidence','Dependency sources under legacy _deps','Final RAX target and retained Assist build-native-attributes for concrete next-frontier reuse'],'basis':'All macOS/Linux drivers and owning consumers completed; no live command refers to either obsolete directory; legacy path absent from final CMake cache/Ninja graph.'}
for p in [old,legacy]:
 try:
  result=subprocess.run(['lsof','-nP','+D',str(p)],capture_output=True,timeout=30)
 except subprocess.TimeoutExpired:raise RuntimeError('Open-file audit did not complete; no cleanup attempted')
 assert result.returncode==1 and not result.stdout,('open files remain',str(p),result.stdout[:1000])
 if p==old:
  shutil.rmtree(p);record['removed'].append(str(p))
 else:
  for child in p.iterdir():
   if child.name=='_deps':continue
   if child.is_dir() and not child.is_symlink():shutil.rmtree(child)
   else:child.unlink()
   record['removed'].append(str(child))
(K/'attributes-obsolete-build-cleanup.json').write_text(json.dumps(record,indent=2)+'\n')
print('Removed obsolete task-generated outputs; dependency sources and reusable final builds preserved',flush=True)
