from pathlib import Path
import hashlib,json,shutil,sys,tarfile
ROOT=Path('/Users/int/.assist-session-alias-20261020');CACHE=Path(__file__).parent;DOCS=ROOT/'docs/specifications/windows/native-session-directory-alias';SHARE=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');number=int(sys.argv[1]);assert number in [1,2]
paths={p.relative_to(ROOT).as_posix()for p in DOCS.rglob('*')if p.is_file()};manifest=json.loads((ROOT/'docs/development/evidence-projection/manifest.json').read_text());paths.update(row['path']for row in manifest['files']);paths.update(['tools/evidence/public_projection.py','docs/development/evidence-projection/manifest.json','docs/development/evidence-projection/README.md']);archive=ROOT/'target'/('session-alias-proof-round'+str(number)+'.tar.gz');assert not archive.exists()
with tarfile.open(archive,'w:gz')as out:
 for name in sorted(paths):out.add(ROOT/name,arcname=name,recursive=False)
raw=archive.read_bytes();envelope=DOCS/'evidence-manifest.json';value={'archive':archive.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'files':[{'path':name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}for name in sorted(paths)for data in[(ROOT/name).read_bytes()]],'envelope':{'bytes':envelope.stat().st_size,'sha256':hashlib.sha256(envelope.read_bytes()).hexdigest()}};meta=CACHE/('session-alias-proof-round'+str(number)+'.json');meta.write_text(json.dumps(value,indent=2)+'\n')
for path in [archive,meta]:shutil.copy2(path,SHARE/path.name)
print(number,len(paths),len(raw),flush=True)
