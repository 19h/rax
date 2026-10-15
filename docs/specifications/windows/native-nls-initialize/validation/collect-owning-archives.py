from pathlib import Path
import hashlib,json,argparse

def identity(path):
 h=hashlib.sha256()
 with path.open('rb') as f:
  for data in iter(lambda:f.read(1<<20),b''):h.update(data)
 return {'path':str(path),'bytes':path.stat().st_size,'sha256':h.hexdigest()}

def archive(path):
 row=identity(path);count=0;size=row['bytes']
 with path.open('rb') as f:
  if f.read(8)!=b'!<arch>\n':raise ValueError('archive magic')
  while f.tell()<size:
   header=f.read(60)
   if len(header)!=60 or header[58:]!=b'`\n':raise ValueError('member header')
   length=int(header[48:58]);end=f.tell()+length+(length&1)
   if length<0 or end>size:raise ValueError('member extent')
   f.seek(end);count+=1
  if f.tell()!=size:raise ValueError('archive extent')
 row.update({'members':count,'structural-member-walk':'pass'});return row

def main():
 p=argparse.ArgumentParser();p.add_argument('--host',required=True);p.add_argument('--source',type=Path,required=True);p.add_argument('--manifest',type=Path,required=True);p.add_argument('--target',type=Path,required=True);p.add_argument('--fingerprint',required=True);p.add_argument('--output',type=Path,required=True);args=p.parse_args()
 if not __debug__:raise RuntimeError('optimized validation rejected')
 m=json.loads(args.manifest.read_text());sources=[]
 for row in m['sources']:
  actual=identity(args.source/row['path'])
  if (actual['bytes'],actual['sha256'])!=(row['bytes'],row['sha256']):raise ValueError('actual source '+row['path'])
  sources.append(row)
 fingerprint=args.target/'debug/.fingerprint'/('rax-'+args.fingerprint)/'lib-rax.json'
 core=json.loads(fingerprint.read_text())
 if core['features']!='[]':raise ValueError('empty core feature selection')
 result={'host':args.host,'sources':sources,'core-features':[],'core-fingerprint':identity(fingerprint),'archives':[archive(args.target/'debug/libassist_rs.a'),archive(args.target/'debug/deps'/('librax-'+args.fingerprint+'.rlib'))]}
 args.output.write_text(json.dumps(result,indent=2)+'\n')
 print(args.host,'source units',len(sources),'archives',[(x['bytes'],x['members'],x['sha256']) for x in result['archives']])
if __name__=='__main__':main()
