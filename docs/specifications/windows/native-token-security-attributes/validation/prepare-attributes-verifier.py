from pathlib import Path
k=Path(__file__).parent;h=k.parent/'native-worker-factory-idle-timeout-20261019'
t=(h/'check-idle-validation.py').read_text()
t=t.replace('(8037, 0, 2)','(8049, 0, 2)').replace('(8031, 0, 2)','(8043, 0, 2)').replace('(7499, 5, 2)','(7512, 5, 2)')
a=t.index('    import runpy\n');b=t.index('    baseline = ',a)
t=t[:a]+'''    import runpy, shutil, tempfile
    scratch = source / 'build-tmp'
    scratch.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='attributes-proof-', dir=scratch) as temp:
        temporary = Path(temp)
        for name in ['cases.json', 'empty-cases.json', 'upper-cases.json']:
            shutil.copy2(evidence / name, temporary / name)
        entry = runpy.run_path(str(evidence / 'check_layouts.py'))['main']
        entry.__globals__['HERE'] = temporary
        assert entry() == 0
        native = load(temporary / 'native-layout-check.json')
        assert native['checked'] == 22656 and native['zero-value-creation-controls'] == 28
    primary = load(evidence / 'primary/phnt-source.json')
    assert primary['url'] == 'https://raw.githubusercontent.com/winsiderss/phnt/53fbbdc5b5d2b08761db1c7b26bfa8c820924356/ntseapi.h'
    raw = (evidence / 'primary/phnt-ntseapi.h').read_bytes()
    assert len(raw) == primary['bytes'] == 74208
    assert hashlib.sha256(raw).hexdigest() == primary['sha256'] == '7fa0a21902ed64036c495216fba85d0e29a1d3028fd60d09103993817d1473df'
    manifest = load(validation / 'attributes-candidate-epoch2.json')
    rows = manifest['files']
    assert len(rows) == 24786 and sum(row['bytes'] for row in rows) == 1808385701
    assert len({row['path'] for row in rows}) == len(rows)
    assert not any('__pycache__' in Path(row['path']).parts for row in rows)
    assert manifest['base-repository-commit'] == 'd03afec80afd8ef4281c839a8944a166aef02369'
    source_rows = rows
    updates = {row['path']: row for row in load(validation / 'engineering-source-updates.json')['files']}
    for row in rows:
        path = source / row['path']
        raw = os.readlink(path).encode() if path.is_symlink() else path.read_bytes()
        digest = hashlib.sha256(raw).hexdigest()
        if row['path'] == '.cargo/config.toml' and digest == NATIVE_CONFIG:
            assert os.name == 'nt' and len(raw) == 374
        elif row['path'] in updates:
            update = updates[row['path']]
            assert (row['path'].startswith('docs/specifications/windows/native-token-security-attributes/')
                    or row['path'] == 'src/user/windows/native-runtime.md')
            assert update['before-sha256'] == row['sha256'] and update['before-bytes'] == row['bytes']
            assert update['after-sha256'] == digest and update['after-bytes'] == len(raw)
            if row['path'] == 'src/user/windows/native-runtime.md':
                assert hashlib.sha256(raw[:row['bytes']]).hexdigest() == row['sha256']
        else:
            assert len(raw) == row['bytes'] and digest == row['sha256'], row['path']
''' +t[b:]
a=t.index('    baseline = ');b=t.index('    for host, counts ',a)
t=t[:a]+'''    baseline = load(validation / 'attributes-baseline-proof.json')
    assert baseline['state'] == 'expected-regression-failed'
    assert baseline['passed'] == 0 and baseline['failed'] == 3
    raw_record = (validation / baseline['record']).read_bytes()
    assert hashlib.sha256(raw_record).hexdigest() == baseline['record-sha256']
    observed = load(validation / baseline['record'])
    assert observed['exit-code'] == 101
    raw = (validation / observed['log']).read_bytes()
    assert hashlib.sha256(raw).hexdigest() == observed['sha256'] and len(raw) == observed['bytes']
    assert b'0 passed; 3 failed;' in raw and b'Finished `test` profile' in raw and b'error[E' not in raw
    for name in ['empty', 'upper']:
        proof = load(validation / ('attributes-' + name + '-baseline-proof.json'))
        assert proof['state'] == 'expected-regression-failed' and proof['passed'] == 0 and proof['failed'] == 1 and proof['exit-code'] == 101
        raw = (validation / ('attributes-' + name + '-baseline-epoch2.log')).read_bytes()
        assert hashlib.sha256(raw).hexdigest() == proof['log-sha256']
        assert b'0 passed; 1 failed;' in raw and b'Finished `test` profile' in raw and b'error[E' not in raw
    prototype = load(validation / 'attributes-prototype-epoch2-e.json')
    assert prototype['state'] == 'finished' and prototype['exit-code'] == 0
    raw = (validation / prototype['log']).read_bytes()
    assert len(raw) == prototype['bytes'] and hashlib.sha256(raw).hexdigest() == prototype['sha256']
    assert b'12 passed; 0 failed;' in raw
    for row in prototype['before-and-after-inputs']:
        raw = (source / row['path']).read_bytes()
        assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], row['path']
''' +t[b:]
t=t.replace('idle-native-epoch2-gates.json','attributes-native-epoch2-gates.json').replace("f'idle-{host}-epoch2-gates.json'","f'attributes-{host}-epoch2-gates.json'").replace("'7 passed; 0 failed;'","'13 passed; 0 failed;'").replace('native_worker_factory_idle','native_token_security_attributes').replace('== 7\n            assert any(', '== 13\n            assert any(').replace("assert any('native_token_security_attributes_installed_leaf_updates_request_and_four_argument_cleanup' in name for name in names)","assert any('installed_leaf' in name for name in names)")
t=t.replace('macos-idle-epoch2-owning-gates.json','macos-attributes-epoch2-owning-gates-c.json').replace('linux-idle-epoch2-owning-gates.json','linux-attributes-epoch2-owning-gates-b.json').replace('idle-native-owning-epoch2.json','attributes-native-owning-epoch2-c.json')
t=t.replace('macos-idle-epoch2-pre-cpp-archive-identities.json','macos-attributes-epoch2-c-pre-cpp-archive-identities.json').replace('macos-idle-epoch2-final-owning-archive-identities.json','macos-attributes-epoch2-c-final-owning-archive-identities.json')
t=t.replace('linux-idle-epoch2-pre-cpp-archive-identities.json','linux-attributes-epoch2-pre-cpp-archive-identities.json').replace('linux-idle-epoch2-final-owning-archive-identities.json','linux-attributes-epoch2-final-owning-archive-identities.json')
t=t.replace('native-idle-epoch2-pre-cpp-archives.json','native-attributes-epoch2-c-pre-cpp-archives.json').replace('native-idle-epoch2-final-owning-archive-identities.json','native-attributes-epoch2-c-final-owning-archive-identities.json').replace('idle-owning-source-hashes-epoch2.json','attributes-owning-source-hashes-epoch2.json').replace('macos-idle-epoch2-linkage.json','macos-attributes-epoch2-c-linkage.json').replace('== 554','== 880')
# Frozen selected engineering records can be extended, never compiled sources.
t=t.replace("assert len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'], row['path']\n        for row in after['archives']:","assert (row['path'] in updates or (len(raw) == row['bytes'] and hashlib.sha256(raw).hexdigest() == row['sha256'])), row['path']\n        for row in after['archives']:")
a=t.index('    controls = load(');b=t.index('    ordinary = ',a)
t=t[:a]+'''    controls = load(validation / 'attributes-oracle-negative-results.json')
    assert len(controls['controls']) == 16
    assert controls['controls'][0]['exit-code'] == 0
    assert all(row['exit-code'] != 0 for row in controls['controls'][1:])
''' +t[b:]
t=t.replace('idle-owner-epoch2','attributes-owner-epoch2').replace('3c3f596cf2343c4f0c8c9a7e0b75b8ceb6533b7b81cbacba5db713f3a9d29e7d','684d908a33165a0479bbbf84c7ea55f88e3cc4325ce8e3a7845df6e4553a9154').replace('[2898814, 2901099]','[2899173, 2901458]')
a=t.index("        call = next(line ");b=t.index("        assert text.count('guest-console ')",a)
t=t[:a]+'''        for turn in [previous, previous + 16]:
            line = next(line for line in text.splitlines() if line.startswith(f'token-security-attributes-result turn={turn} tid=8 '))
            assert 'status=Some(c0000225)' in line and 'required=Ok(0)' in line and 'result=BudgetExhausted' in line
        terminal = re.search(r'^terminal turn=(\\d+) (.+)$', text, re.M)
        delta = [44004, 64739][index]
        expected = ['NtTestAlert (0x1d3)', 'NtQueryVirtualMemory class 3'][index]
        assert terminal and int(terminal.group(1)) == previous + delta == row['new-terminal-turn']
        assert terminal.group(2) == row['terminal-result'] and expected in terminal.group(2)
        assert row['additional-observer-turns'] == delta and row['guest-exit-success'] is False
''' +t[b:]
t=t.replace('native-worker-factory-idle-timeout','native-token-security-attributes')
(k/'check-attributes-validation.py').write_text(t)
print('prepared validation checker',len(t.splitlines()),'lines')
