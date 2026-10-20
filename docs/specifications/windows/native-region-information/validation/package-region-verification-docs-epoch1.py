from pathlib import Path
import hashlib,json,shutil,sys,tarfile
P=Path(__file__).parent;R=Path('/Users/int/.assist-region-20261020');D=R/'docs/specifications/windows/native-region-information';S=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154');number=int(sys.argv[1]);assert number in [1,2];archive=R/'target'/('region-epoch1-verification-docs-round'+str(number)+'.tar.gz');assert not archive.exists();files=[]
with tarfile.open(archive,'w:gz')as tar:
 for path in sorted(D.rglob('*')):
  if path.is_file():
   name=str(path.relative_to(R));raw=path.read_bytes();files.append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()});tar.add(path,arcname=name,recursive=False)
meta={'archive':archive.name,'bytes':archive.stat().st_size,'sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'files':files,'envelope':{'bytes':(D/'evidence-manifest.json').stat().st_size,'sha256':hashlib.sha256((D/'evidence-manifest.json').read_bytes()).hexdigest()}}
shutil.copy2(archive,S/archive.name);path=P/('region-epoch1-verification-docs-round'+str(number)+'.json');path.write_text(json.dumps(meta,indent=2)+'\n');shutil.copy2(path,S/path.name);print(number,len(files),meta['bytes'])
