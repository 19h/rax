"""Reject altered final claims after binding metadata digests again."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile

D=Path(__file__).resolve().parent
P=D.parents[3]

def load(p):
    raw=p.read_bytes()
    return json.loads(raw.decode('utf-16') if raw[:2] in [b'\xff\xfe',b'\xfe\xff'] else raw.decode('utf-8-sig'))

def save(p,v):p.write_text(json.dumps(v,indent=2)+'\n')

def edit_json(root,name,mutate):
    p=root/'validation'/name;v=load(p);mutate(v);save(p,v)

def edit_text(root,name,before,after):
    p=root/'validation'/name;t=p.read_text(encoding='utf-8-sig')
    if before not in t:raise ValueError('missing stress target')
    p.write_text(t.replace(before,after,1))

def fake_guest_exit(root):
    owner=load(root/'validation/links-owner-observer-launch.json')
    row=owner['outcomes'][0];p=root/'validation'/row['log'];t=p.read_text()
    lines=t.splitlines();lines=[('terminal turn=2150070 Complete(Exited(0))' if l.startswith('terminal ') else l)for l in lines]
    p.write_text('\n'.join(lines)+'\n');raw=p.read_bytes()
    row['log-identity'].update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
    save(root/'validation/links-owner-observer-launch.json',owner)

cases=[
    ('changed-source',lambda r:edit_json(r,'links-source-epoch1.json',lambda v:v['files'][0].update(sha256='0'*64))),
    ('new-native-failure',lambda r:edit_text(r,'links-native-epoch1-full.log','    smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime','    unrecorded_failure')),
    ('hidden-ignore',lambda r:edit_text(r,'links-native-epoch1-full.log','test smir::lower::validation::tests::test_lift_lower_full_microkernel ... ignored','test smir::lower::validation::tests::test_lift_lower_full_microkernel ... ok')),
    ('enabled-core-defaults',lambda r:edit_json(r,'native-links-final-owning-archive-identities.json',lambda v:v.update({'core-features':['default']}))),
    ('wrong-actual-artifact',lambda r:edit_json(r,'native-links-final-owning-archive-identities.json',lambda v:v['core-artifact'].update(package_id='unrecorded'))),
    ('false-current-equality',lambda r:edit_json(r,'links-upstream-source-comparison.json',lambda v:v['files'][0].update(equal=False))),
    ('invented-guest-exit',fake_guest_exit),
    ('missing-all-target-check',lambda r:edit_json(r,'links-native-epoch1-gates.json',lambda v:v.pop())),
]
results=[]
for name,mutate in cases:
    with tempfile.TemporaryDirectory(prefix='links-validation-stress-') as tmp:
        root=Path(tmp)/'proof';shutil.copytree(D,root,ignore=shutil.ignore_patterns('__pycache__'))
        mutate(root)
        manifest=load(root/'validation/inputs.json')
        for row in manifest['files']:
            raw=(root/row['path']).read_bytes();row.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
        save(root/'validation/inputs.json',manifest)
        p=subprocess.run([sys.executable,'-B',str(root/'check-validation.py'),'--source-root',str(P)],capture_output=True,text=True)
        if p.returncode==0:raise RuntimeError('accepted rebound alteration '+name)
        results.append({'case':name,'rejected':True,'error':p.stderr.splitlines()[-1]})
print(json.dumps(results,indent=2))
