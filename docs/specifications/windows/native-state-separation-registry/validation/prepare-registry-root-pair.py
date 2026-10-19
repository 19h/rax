from pathlib import Path
import json
import subprocess

Q = Path(__file__).parent
ROOT = Q / 'assist-root-registry-final'
RAX = Q / 'registry-source'
MAIN = Path('/Users/int/hexrays/kvasir')
commit = json.loads((Q / 'registry-rax-commit.json').read_text())['commit']

def git(root, *args):
    return subprocess.check_output(['git', *args], cwd=root, text=True).strip()

assert git(RAX, 'ls-remote', 'origin', 'refs/heads/master').split()[0] == commit
orig = git(MAIN, 'ls-remote', 'origin', 'refs/heads/master').split()[0]
mirror = git(MAIN, 'ls-remote', '19h', 'refs/heads/master').split()[0]
assert orig == mirror, 'Inspect root remote divergence before creating a paired pin.'
assert not ROOT.exists(), 'Inspect an earlier root pair; do not recreate it.'
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
record = ROOT / 'VENDORED_VERSIONS.md'
text = record.read_text()
old = f'| Pinned commit (ABI anchor) | `{old_pin}` |'
assert text.count(old) == 1
text = text.replace(old, f'| Pinned commit (ABI anchor) | `{commit}` |', 1)
append = f'''

## Native StateSeparation registry capture (2026-10-19)

RAX `{commit}` snapshots the fixed SYSTEM
`StateSeparation\\RedirectionMap\\Keys` subtree during native-library selection.
Present keys retain complete bounded descendants; only fixed-root
ERROR_FILE_NOT_FOUND establishes known absence. Other acquisition errors and
disappearing enumerated children fail capture. Guest handles reference immutable
records, with no forwarding of guest names to native registry operations. IFEO
uses the same fixed-root model and the aggregate value budgets remain shared.

| Host | Full core passed/failed/ignored | New tests | C API |
|---|---|---|---|
| macOS ARM64 | 7976/0/2 | 6 | 183 |
| Linux x86-64 container | 7970/0/2 | 6 | 183 |
| Windows ARM64 | 7419/5/2 | 9 | 183 |

Windows retains exactly the five existing AArch64 lowerer failures. The native
matrix records 256 cases across four ABI profiles on one ARM64 kernel, with 224
inside the selected root and 32 outside-prefix controls. Producer/module/source
identities and process handle counts are stable. The unchanged implementation
fails the native regression (0 passed, 1 failed); an earlier guest-disk compile
failure is preserved separately. Controlled present acquisition independently
checks 54 keys, 106 values and 4,756 charged name-plus-data bytes without creating
a system registry key. The exact error, scope, budget and lifetime cases pass.

The complete source snapshot binds 22,779 inputs and 1,583,474,670 bytes.
All-target checks and actual retained Assist core/carrier consumer validation
pass on all three hosts, with 513 selected inputs and complete archive walks
before/after C++ validation. These consumers retain Assist Rust ABI184; eight
C++ inputs were compared with root ABI264. The paired publication parent is
`{orig}`. This does not establish whole-current-root GUI/plugin/package
builds. RAX C API 1.14.0 remains unchanged by this registry feature.

Both ordinary ARM64 programs return the native missing-key status at the former
registry boundary and advance another 9,296 observer turns. They reach the
separate unsupported NtSetTimer2 (0x1C0) service without a successful guest exit.
The exact owning Windows core is 503,963,010 bytes, SHA-256
`790ba7cd4d87fe35b63fdbb4aff228cb0b3996a3a2583d09537139fc029db05e`.
The full process-emulation goal remains incomplete.

[Contract and evidence](vendor/rax/docs/specifications/windows/native-state-separation-registry/README.md);
[verification entry point](vendor/rax/docs/specifications/windows/native-state-separation-registry/check_validation.py).
'''
record.write_text(text + append)
doc = ROOT / 'src/emulation/native-process-emulation.md'
doc.write_text(doc.read_text() + append.replace('(vendor/rax/docs/', '(../../vendor/rax/docs/'))
owned = ['VENDORED_VERSIONS.md', 'src/emulation/native-process-emulation.md', 'vendor/rax']
meta = {'parent': orig, 'old-pin': old_pin, 'rax': commit, 'paths': owned,
        'scope': 'New isolated worktree; shared main HEAD, index and user-owned CI files are preserved.'}
(Q / 'registry-root-preparation.json').write_text(json.dumps(meta, indent=2) + '\n')
print('paired root prepared', orig, commit)
