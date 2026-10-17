"""Reject edited, incomplete or overstated WNF validation records."""
from pathlib import Path
import contextlib,hashlib,io,json,shutil,tempfile
B=Path(__file__).resolve().parent;R=B/'rax-source/docs/specifications/windows/native-wnf-query'
def main():
 scope={'__name__':'wnf_proof_stress','__file__':str(R/'validation-check.py')}
 exec(compile((R/'validation-check.py').read_text(),str(R/'validation-check.py'),'exec'),scope)
 results=[]
 for case in ['raw-corruption','empty-native-gates','forged-zero-failures','incomplete-owning-driver','missing-final-unix-gates']:
  with tempfile.TemporaryDirectory(prefix='wnf-proof-')as folder:
   v=Path(folder)/'validation';shutil.copytree(R/'validation',v);m=json.loads((v/'results.json').read_text())
   def bind(name):
    p=v/name;rows=[x for x in m['files']if x['path']==name]
    if len(rows)!=1:raise ValueError('missing record '+name)
    rows[0].update(bytes=p.stat().st_size,sha256=hashlib.sha256(p.read_bytes()).hexdigest())
   if case=='raw-corruption':
    p=v/'windows/native-wnf-query-final4-full.log';p.write_bytes(p.read_bytes()+b'corrupt\n')
   elif case=='empty-native-gates':
    name='windows/native-wnf-query-final4-verified-gates.json';p=v/name;r=json.loads(p.read_text());r['gates']=[];p.write_text(json.dumps(r));bind(name)
   elif case=='forged-zero-failures':
    name='windows/native-wnf-query-final4-full.log';p=v/name;raw=p.read_bytes()
    if b'7287 passed; 5 failed;'not in raw:raise ValueError('missing expected complete native result')
    p.write_bytes(raw.replace(b'7287 passed; 5 failed;',b'7292 passed; 0 failed;'));bind(name)
    name='windows/native-wnf-query-final4-verified-gates.json';p=v/name;r=json.loads(p.read_text())
    for gate in r['gates']:
     if gate['gate']=='full':gate['sha256']=hashlib.sha256((v/'windows/native-wnf-query-final4-full.log').read_bytes()).hexdigest()
    p.write_text(json.dumps(r));bind(name)
   elif case=='incomplete-owning-driver':
    name='windows/wnf-query-final4-launch.json';p=v/name;r=json.loads(p.read_text());r['state']='running-owning';p.write_text(json.dumps(r));bind(name)
   else:
    name='linux-wnf-final4-gates.json';(v/name).write_text('[]\n');bind(name)
   if case!='raw-corruption':(v/'results.json').write_text(json.dumps(m))
   scope['V']=v
   try:
    with contextlib.redirect_stdout(io.StringIO()):scope['main']()
   except Exception as error:results.append({'case':case,'rejected':True,'exception':str(error)})
   else:raise ValueError('accepted altered evidence '+case)
 (B/'wnf-validation-stress-results.json').write_text(json.dumps(results,indent=2)+'\n');print(json.dumps(results))
if __name__=='__main__':main()
