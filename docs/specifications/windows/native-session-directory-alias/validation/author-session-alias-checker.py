from pathlib import Path
ROOT=Path('/Users/int/.assist-session-alias-20261020')
DOCS=ROOT/'docs/specifications/windows/native-session-directory-alias'
old=(ROOT/'docs/specifications/windows/native-namespace-diagnostics/check_validation.py').read_text()
replacements={
'native-namespace-diagnostics':'native-session-directory-alias',
'157ef1e5c974de0148c41b6db5a0c32783bda36d':'6f2148297acc1e8106600306cc092b08207b80d2',
'native-namespace-case-v1-launch.json':'native-session-alias-v1-launch.json',
'native-case-capture-source.json':'session-alias-native-source.json',
'native-namespace-case-v1.cpp':'native-session-alias-v1.cpp',
'namespace-kernel-profile.json':'kernel-profile.json',
'namespace-source-epoch1.json':'session-alias-source-epoch1.json',
'namespace-diagnostic-baseline.json':'session-alias-baseline-epoch2.json',
'namespace-diagnostic-focus.json':'session-alias-focus-epoch1.json',
'namespace-owning-source-hashes-epoch1.json':'session-alias-owning-source-hashes-epoch1.json',
'macos-namespace-epoch1-linkage.json':'macos-session-alias-epoch1-linkage.json',
'namespace-ownership.json':'assumptions-final.json',
"'namespace-cause-on-x64-runner'":"'ci-session-alias-target'",
'25071':'25085','1834294123':'1835122521','3812':'3826',
"('macos', 8083), ('linux', 8077), ('windows', 7544)":"('macos', 8102), ('linux', 8096), ('windows', 7564)",
"'missing selected_path='":"'selected NT directory parent type/name changed'",
"'1 passed; 0 failed;' in text(focus)":"'3 passed; 0 failed;' in text(focus)",
"{'focus': 1, 'capi': 183":"{'focus': 4 if platform == 'windows' else 3, 'capi': 183",
}
for a,b in replacements.items():old=old.replace(a,b)
# New envelopes bind public bytes, while producer receipts and frozen inputs
# retain original checkpoint identities. Both are explicit, exact bindings.
old=old.replace("    assert identity(path) == (row['bytes'], row['sha256']), str(path)","    assert PROJECTION.actual_identity(path) == (row['bytes'], row['sha256']) or identity(path) == (row['bytes'], row['sha256']), str(path)")
needle="        ending = fields(lines[-1])"
extra="""        symbolic = [fields(line) for line in lines if line.startswith('entry role=private-with-symbolic ')]
        assert {(decode(row['name']), decode(row['type'])) for row in symbolic} == {
            ('MiXeD', 'Directory'), ('DirectoryLink', 'SymbolicLink')}
        names = {fields(line)['role']: decode(fields(line)['name']) for line in lines if line.startswith('object ')}
        assert names['followed-directory'] == names['private-child']
        assert names['symbolic-link'] != names['private-child']
        comparison = [fields(line) for line in lines if line.startswith('identity ')]
        assert comparison == [{'same': '00000000', 'different': 'C00001AC', 'canonical-same': '1', 'link-name-distinct': '1'}]
        denied = [fields(line) for line in lines if line.startswith('dont-reparse ')]
        assert denied == [{'status': 'C000050B', 'opened': '0'}]
"""
assert needle in old;old=old.replace(needle,extra+needle)
# Independent current core/CAPI observers complement the retained link gates.
mri=(ROOT/'docs/specifications/windows/native-region-information/check_validation.py').read_text();begin=mri.index('def consumers(source):');end=mri.index('\n\ndef check(',begin);consumer=mri[begin:end].replace('region-owning','session-alias-owning').replace('macos-region','macos-session-alias').replace('region-ordinary-owner-epoch1','session-alias-ordinary-owner-epoch2').replace('3823','3826')
consumer=consumer.replace("assert 'reached-continue=1 reached-regions=0 terminal=true' in smoke", "assert 'terminal-call=974' in smoke")
a=consumer.index("    assert whoami.count(");b=consumer.index("    for outcome in ordinary['outcomes']:",a)
consumer=consumer[:a]+"    assert 'NtQueryAttributesFile (0x3d)' in whoami and 'terminal-call=2179' in whoami\n"+consumer[b:]
consumer=consumer.replace('log(', 'text(')
old=old.replace('\n\ndef check(host, source=SOURCE):','\n\n'+consumer+'\n\ndef check(host, source=SOURCE):')
needle="    assumptions = load(VALIDATION / 'assumptions-final.json')['assumptions']"
assert needle in old;old=old.replace(needle,"    consumers(source)\n"+needle)
old=old.replace("    assert summary['full-process-goal-complete'] is False", "    assert summary['full-process-goal-complete'] is False\n    assert summary['whoami-frontier'] == 'NtQueryAttributesFile0x3d'\n    assert summary['independent-x64-kernel-validated'] is False")
(DOCS/'check_validation.py').write_text(old)
print('checker lines',len(old.splitlines()))
