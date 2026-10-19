from pathlib import Path
import json
import subprocess

Q = Path(__file__).resolve().parent
ROOT = Q / 'assist-root-attributes-final'
RAX = Path('/Users/int/.assist-token-20261019')
MAIN = Path('/Users/int/hexrays/kvasir')
commit = json.loads((Q / 'attributes-rax-commit.json').read_text())['commit']


def git(root, *args):
    return subprocess.check_output(['git', *args], cwd=root, text=True).strip()


assert git(RAX, 'ls-remote', 'origin', 'refs/heads/master').split()[0] == commit
orig = git(MAIN, 'ls-remote', 'origin', 'refs/heads/master').split()[0]
assert git(MAIN, 'ls-remote', '19h', 'refs/heads/master').split()[0] == orig
assert not ROOT.exists()
subprocess.run(['git', 'fetch', 'origin', 'master'], cwd=MAIN, check=True)
assert git(MAIN, 'rev-parse', 'origin/master') == orig
subprocess.run(['git', 'worktree', 'add', '--detach', str(ROOT), orig], cwd=MAIN, check=True)
assert not git(ROOT, 'status', '--porcelain=v1', '--untracked-files=all')
old_pin = git(ROOT, 'ls-tree', 'HEAD', 'vendor/rax').split()[2]
subprocess.run(['git', 'merge-base', '--is-ancestor', old_pin, commit], cwd=RAX, check=True)
submodule = ROOT / 'vendor/rax'
assert not (submodule / '.git').exists()
subprocess.run(['git', 'clone', '--shared', '--no-checkout', str(RAX), str(submodule)], check=True)
subprocess.run(['git', 'remote', 'set-url', 'origin', 'git@github.com:HexRaysSA/rax.git'], cwd=submodule, check=True)
subprocess.run(['git', 'fetch', 'origin', 'master'], cwd=submodule, check=True)
subprocess.run(['git', 'checkout', '--detach', commit], cwd=submodule, check=True)
assert 'pub const RAX_API_MINOR: u32 = 14;' in (submodule / 'capi/src/lib.rs').read_text()
assert '#define RAX_API_MINOR 14' in (submodule / 'capi/include/rax.h').read_text()
summary = json.loads((RAX / 'docs/specifications/windows/native-token-security-attributes/validation/summary.json').read_text())
core = summary['ordinary-programs']['actual-core']
append = '\n\n## Captured token security attributes publication (2026-10-19)\n\nRAX `' + commit + '`; paired publication parent `' + orig + '`.\n' + (Q / 'root-runtime-append.md').read_text() + '\n[Evidence and assumptions](vendor/rax/docs/specifications/windows/native-token-security-attributes/README.md); [portable verifier](vendor/rax/docs/specifications/windows/native-token-security-attributes/check_validation.py).\n'
record = ROOT / 'VENDORED_VERSIONS.md'
text = record.read_text()
old = f'| Pinned commit (ABI anchor) | `{old_pin}` |'
assert text.count(old) == 1
record.write_text(text.replace(old, f'| Pinned commit (ABI anchor) | `{commit}` |', 1) + append)
doc = ROOT / 'src/emulation/native-process-emulation.md'
old_text = doc.read_text()
rows = old_text.splitlines()
matching = [row for row in rows if row.startswith('| Windows on Windows uses installed DLLs |')]
assert len(matching) == 1
new_row = '| Windows on Windows uses installed DLLs | Installed NTDLL/native-WoW leaves and retained Assist owning consumers pass the measured contracts. The latest token-attribute group lets ordinary ARM64 smoke and whoami pass missing SYSAPPID/PKG queries and advance44004/64739 observer turns to unsupported NtTestAlert / NtQueryVirtualMemory class3. Native controls exit0; neither guest exits successfully. The detailed milestones, source/archive/runtime identities and bounded kernel scope are recorded below and in the paired RAX evidence. | Native selection and measured startup continuation demonstrated; ordinary Windows process completion remains incomplete |'
doc.write_text(old_text.replace(matching[0], new_row, 1) + append.replace('(vendor/rax/docs/', '(../../vendor/rax/docs/'))
(Q / 'attributes-root-preparation.json').write_text(json.dumps({
    'parent': orig, 'old-pin': old_pin, 'rax': commit,
    'paths': ['VENDORED_VERSIONS.md', 'src/emulation/native-process-emulation.md', 'vendor/rax'],
    'scope': 'New isolated worktree; shared main HEAD/index/user CI files preserved.',
}, indent=2) + '\n')
print('paired root prepared', orig, commit)
