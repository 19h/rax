# Completed constructor validation

The frozen input identity is base51aa9b11 plus the nine Rust edits listed in
owned-source-hashes.json, not a claim that the constructor existed in that base.
section-create-sources.json records8,519 input files before the gates. This
validation directory and its checker were added after those runs.

| Actual host | Library passed/failed/ignored | Constructor tests | C API |
|---|---|---|---|
| macOS ARM64 | 7804/0/2 | 6/0 | 168/0 |
| Linux x86-64 container | 7798/0/2 | 6/0 | 168/0 |
| Windows ARM64 | 7224/5/2 | 7/0 | 168/0 |

All-target checks pass on all three. The five Windows failures are the unchanged
BZHI/FP16 lowerer baseline, individually named in summary.json. Windows native
memory tests run four cases; the Unix configurations select no such tests.
Linux integration passes42 with one ignored on each Unix host; Windows
personality integration passes544 on each Unix host. Linux-host Darwin tests
are platform-gated and do not establish native Darwin behavior.

Scoped owning validation passes five C++ consumers on Linux and Windows.
macOS links the shipping CLI/MCP, passes seven CTests and two protected-text
scans. Each embedding verifies270 source/fixture identities, complete archive
member walks, and empty core features. owning-root-scope.json records the older
root exports and nine current matching C++ consumers. These results do not
establish whole-current-root plugin, GUI or package validation.

The unchanged11,178-byte Rust observer now stops at NtSecureConnectPort0x194,
PC0x180002980, turn2,115,163. The previous constructor frontier was turn2,115,129:
2,115,163 - 2,115,129 =34 scheduler calls. Ordinary guest exits remain0.
The original C++ probe retains its5,000ms per-program wall-clock budget and
reports RAX_PROCESS_TIMEOUT6 for three CRT samples. A separately retained
60,000ms probe reaches explicit NtSecureConnectPort failure for all three.
The whoami frontier remains NtTraceEvent0x5e. A zero exit-code field attached
to timeout/failure is not a successful guest exit.

Final assumption reconciliation is in summary.json: C1 remains limited to an
ARM64 kernel and compatibility execution, C2 confirms bounded construction,
and C3 confirms native host backings and ownership across all three systems.
The initial README/register describe preparation-time state; this record
supplies completed validation. Unsupported mapping/query/security/mode paths
and broader Darwin pressure behavior remain explicit, separate work.

Quality gates for this semantic group: QG1 no normative judgment required;
QG2 registered assumptions reconciled; QG3 bounded constructor and all three
platform gates covered; QG4 byte/page/timeout arithmetic explicit; QG5 failure,
timeout, ignored and unsupported outcomes retained; QG6 native producer/input,
raw output and source hashes checked; QG7 findings scoped separately. This
group does not complete the full process-userland goal.

Run check.py, check_rejections.py and check_validation.py without Python -O.
Raw transferred logs and reference HTML retain their original bytes, including
CRLF and trailing blank lines; canonical source/metadata is checked separately.
