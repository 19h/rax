from pathlib import Path
import datetime,hashlib,io,json,shutil,tarfile,time
Q=Path(__file__).parent;R=Q/'registry-source';D=R/'docs/specifications/windows/native-state-separation-registry'
S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
rows=[]
for p in sorted(D.rglob('*')):
 if not p.is_file() or p.name=='evidence-manifest.json':continue
 assert '__pycache__' not in p.parts and not p.is_symlink()
 raw=p.read_bytes();rows.append({'path':p.relative_to(D).as_posix(),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
envelope={'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'files':rows,'scope':'Bound evidence files; manifest excludes itself. Verification receipts added later are documented with their original checked-envelope scope.'}
(D/'evidence-manifest.json').write_text(json.dumps(envelope,indent=2)+'\n')
archive=Q/'registry-final-proof.tar.gz'
assert not archive.exists(),'Do not overwrite the sent proof archive; inspect and version a new attempt if required.'
paths=[p for p in sorted(D.rglob('*')) if p.is_file()]+[R/'src/user/windows/native-runtime.md']
with tarfile.open(archive,'w:gz') as tar:
 for p in paths:
  raw=p.read_bytes();info=tarfile.TarInfo(p.relative_to(R).as_posix());info.size=len(raw);info.mode=0o644;info.mtime=int(time.time());tar.addfile(info,io.BytesIO(raw))
meta={'archive':archive.name,'bytes':archive.stat().st_size,'sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'bound-evidence-files':len(rows),'members':len(paths),'scope':'Final evidence and appended runtime engineering record only; native verifier detaches all overwritten hardlinks before extraction.'}
for base in [Q,S]:(base/'registry-final-proof-archive.json').write_text(json.dumps(meta,indent=2)+'\n')
shutil.copy2(archive,S/archive.name)
print(json.dumps(meta,indent=2))
