"""Run adversarial evidence checks in temporary copies; never change raw proof."""
from pathlib import Path
import contextlib,hashlib,io,json,shutil,tempfile,traceback
B=Path(__file__).resolve().parent
R=B/'rax-final-source/docs/specifications/windows/native-cookie-owner'

def run():
 scope={'__name__':'cookie_stress','__file__':str(R/'validation-check.py')}
 exec(compile((R/'validation-check.py').read_text(),str(R/'validation-check.py'),'exec'),scope)
 cases=[]
 for kind in ['raw-corruption','empty-native-gates','forged-zero-failures','incomplete-owning-driver']:
  with tempfile.TemporaryDirectory(prefix='cookie-evidence-') as temp:
   v=Path(temp)/'validation';shutil.copytree(R/'validation',v)
   manifest=json.loads((v/'results.json').read_text())
   if kind=='raw-corruption':
    n='windows/native-cookie-owner-final-full.log';p=v/n;p.write_bytes(p.read_bytes()+b'corruption\n')
   else:
    if kind=='empty-native-gates':
     n='windows/native-cookie-owner-final-verified-gates.json';p=v/n;x=json.loads(p.read_text());x['gates']=[];p.write_text(json.dumps(x))
    elif kind=='forged-zero-failures':
     n='windows/native-cookie-owner-final-full.log';p=v/n;p.write_bytes(p.read_bytes().replace(b'7263 passed; 5 failed;',b'7268 passed; 0 failed;'))
    else:
     n='windows/cookie-owner-final-owning-restored.json';p=v/n;x=json.loads(p.read_text());x['state']='running';p.write_text(json.dumps(x))
    for row in manifest['files']:
     if row['path']==n:row.update(bytes=p.stat().st_size,sha256=hashlib.sha256(p.read_bytes()).hexdigest())
    if kind=='forged-zero-failures':
     gate_name='windows/native-cookie-owner-final-verified-gates.json';gate_path=v/gate_name
     gates=json.loads(gate_path.read_text())
     for gate in gates['gates']:
      if gate['gate']=='full':gate['sha256']=hashlib.sha256(p.read_bytes()).hexdigest()
     gate_path.write_text(json.dumps(gates))
     for row in manifest['files']:
      if row['path']==gate_name:row.update(bytes=gate_path.stat().st_size,sha256=hashlib.sha256(gate_path.read_bytes()).hexdigest())
    (v/'results.json').write_text(json.dumps(manifest))
   scope['V']=v
   try:
    with contextlib.redirect_stdout(io.StringIO()):scope['main']()
   except Exception as e:cases.append({'case':kind,'rejected':True,'exception':str(e)})
   else:raise ValueError('accepted corrupted evidence '+kind)
 (B/'cookie-validation-stress-results.json').write_text(json.dumps(cases,indent=2)+'\n')
 print(json.dumps(cases))
if __name__=='__main__':run()
