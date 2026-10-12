from pathlib import Path
import hashlib,json,re,shutil

base=Path(__file__).parent
evidence=Path('/Users/int/hexrays/kvasir/vendor/rax/docs/specifications/windows/native-wait-packet-associate')
share=Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
for path in share.glob('native-wait-packet-associate-*'):
    if path.suffix in {'.log','.json','.jsonl'}:
        shutil.copy2(path,base/path.name)
for path in share.glob('assist-native-wait-packet-associate-*.log'):
    shutil.copy2(path,base/path.name)

full={}
for host,prefix in [('macos','macos-'),('linux','linux-'),('windows','native-wait-packet-associate-')]:
    name=(host+'-gates.json') if host!='windows' else 'native-wait-packet-associate-gates.json'
    gates=json.loads((base/name).read_text(encoding='utf-8-sig'))
    if len(gates)!=11:raise ValueError('incomplete gates '+host)
    text=(base/(prefix+'full.log')).read_text(encoding='utf-8-sig')
    matches=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',text)
    if len(matches)!=1:raise ValueError('full-suite stdout '+host)
    full[host]=tuple(map(int,matches[0]))

summary=json.loads((base/'validation-summary.json').read_text())
summary['tests']['full']={host:dict(zip(['pass','fail','ignored'],counts)) for host,counts in full.items()}
summary['limitations']=[
    'NtRemoveIoCompletion and worker-factory mutation/activation remain separate unimplemented services.',
    'Linux full-suite timeout failures and initial-attempt failures are retained; all three unchanged isolated timeout cases pass.',
    'Windows four BZHI and one FP16 lowering failures are retained; the initial thread-clock failure and its isolated passing run are also retained.',
    'Cause of timing-result variation is unknown.',
    'Mixed ordinary-wait/association fairness is not measured; Windows auto-reset event thread selection has no FIFO guarantee.',
    'Native pointer counts vary; the cause and exact kernel reference ownership counts are unknown.',
    'No physical Windows x64 execution proof; Linux amd64 execution uses CPU translation.'
]
(base/'validation-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
for path in list(evidence.iterdir()):
    candidate=base/path.name
    if path.is_file() and candidate.is_file():shutil.copy2(candidate,path)
for path in list(base.glob('linux-final-isolated-*'))+list(base.glob('linux-integration-parallel-*'))+[base/'linux-gates-before-serial-integration.json']:
    shutil.copy2(path,evidence/path.name)
for name in ['native-final-gates-driver.log','native-final-owning-driver.log','finalize-validation.py']:
    shutil.copy2(base/name,evidence/name)

checker=evidence/'check_native_wait_packet_associate.py'
text=checker.read_text()
text=re.sub(r"full=\{[^\n]+\}", 'full='+repr(full),text)
text=text.replace("(101 if g['gate']=='full' and host!='macos' else 0)","(101 if g['gate']=='full' and full[host][1] else 0)")
checker.write_text(text)

manifest={p.name:{'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(evidence.iterdir()) if p.is_file() and p.name!='evidence-manifest.json'}
(evidence/'evidence-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps({'full':full,'files':len(manifest)}))
