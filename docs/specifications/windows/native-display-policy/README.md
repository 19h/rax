# Installed Display policy registry selection

The installed Windows runtime now captures the fixed root
`\Registry\Machine\Software\Policies\Microsoft\Windows\Display` through the
existing bounded, immutable registry snapshot mechanism. The preceding actual
Assist callers stopped at `NtOpenKey` for this root. Selection now records its
observed absence or stable present subtree. Other acquisition errors propagate.
Guest names and access requests use the existing guest registry implementation.

The implementation adds one `TreeRoot` variant, its fixed path, and the installed
adapter selection. Existing limits, permissions, C API 1.14.0, dependency pins,
feature defaults and public layouts remain unchanged. Parent paths, neighboring
names, prefix collisions, literal slash suffixes and NUL suffixes remain outside
the selected root. Descendant records own their raw data after registry teardown.

## Native evidence and bounds

The source, four compiler/run records, PE observations and image/runtime hashes
are in [native](native). The producer performs 144 opens in each of four views:
ARM64, x64, x86 WoW64 and x86 WoW64 with large-address awareness. Each view spans
four spellings, four attributes masks (`0`, `0x40`, `0x200`, `0x240`) and nine
access masks. Thus `4 * 4 * 9 = 144` calls per view and `4 * 144 = 576` calls per
observation round. The final audit repeats all 576 calls using the same image
bytes. Both rounds run on one ARM64 Windows kernel; the x64 view does not
establish behavior on an independent physical x64 installation.

Every observed open returns `STATUS_OBJECT_NAME_NOT_FOUND` (`0xC0000034`), clears
the failed handle output and preserves `LastError=0x12345678`. Before/after
handle counts agree. These observations establish absence on this installation.
The implementation also supports a present subtree through the existing capture
adapter. Its tests exercise controlled present acquisition using a read-only
native subtree, native presence/view comparisons, root not-found, other errors,
and disappearance of an enumerated child.

Capture retains the existing limits: 4096 aggregate values, 16 MiB aggregate value
bytes, 1024 keys per selected tree, depth 32, 255 UTF-16 code units per component,
and 1 MiB aggregate tree path bytes. Stable present acquisition requires two
matching bounded captures. For K keys, V values, P path bytes and B value bytes,
transfer work and retained space are O(K + V + P + B), with the existing per-node
sorting cost added to transfer work. UTF-16 code units occupy 2 bytes; the
preceding request's 122-byte name contains `122 / 2 = 61` code units.

```text
fixed host root -> absent / stable bounded tree / acquisition error
                               |
                    immutable guest registry
                               |
guest NtOpenKey -> existing lookup, access and handle implementation
```

## Assumption register

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| DP-A1 | The root is absent on the validation installation | Two native observation rounds, 1152 opens | This installation's expected status | Controlled present capture; three native adapter tests | A successful native root open requires present capture; another error must propagate | Confirmed for the recorded installation |
| DP-A2 | Selection remains confined to this fixed root | Shared lookup contract and new portable tests | Guest selection boundary | Parents, neighbors, prefixes, separators, NULs, duplicate selection and aggregate budget | An unrelated path becoming selected falsifies confinement | Confirmed for exercised boundaries |
| DP-A3 | The exercised consumers use the frozen implementation | 29015-input ledger, 3969-input owning envelope, selected Cargo artifacts and archive walks | Attribution of results | Source hashes, archive stability, native image/runtime hashes, corruption controls | A source, selected archive or caller hash mismatch falsifies attribution | Confirmed for captured source and consumers |

## Change surface

| Plane | Effect and evidence |
| --- | --- |
| Registry model and installed Windows adapter | Fixed root selection through the existing bounded snapshot; portable and native adapter tests |
| NT dispatch, guest ABIs and object lifetime | Existing dispatch/access/handle implementation; 576 oracle rows replay on each of three guest ABIs, 1728 calls per host |
| Linux/macOS hosts | Shared registry tests compile and run; native acquisition remains in its existing Windows adapter |
| Instruction execution, ISA lowering and optional engines | No implementation changes; existing failures remain reported |
| Assist lifecycle, UI, agent/context, tool schema, permission policy, main-thread dispatch, mesh/transport, persistence and SDK | No changed contract or source; retained actual consumers rebuilt against the selected core |
| ABI, discovery, updates and package layout | No changed layout, capability, resource or dependency; whole current-root products remain separately gated by CI |
| Tests and provenance | Three portable tests, three Windows adapter tests, one replay test, independent parent caller evidence and executable evidence checkers |

## Validation

| Host | Unfiltered library | C API | Windows integration | Actual retained Assist consumers |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 8176 passed, 0 failed, 2 ignored | 183 passed | 544 passed | Eight requested targets built; seven CTests passed |
| Linux x64 container | 8170 passed, 0 failed, 2 ignored | 183 passed | 544 passed | Five C++ consumers passed |
| Windows ARM64 | 7648 passed, five unchanged lowerer failures, 2 ignored | 183 passed | Four passed; companion binary registers zero under native cfg | Five C++ consumers passed |

Focused Display tests pass: four on macOS/Linux and seven on Windows. All-target
checks pass on all three hosts. Rustfmt 1.95 and the Clippy all-target check with
`x86_64-suite` pass. The Windows full receipt preserves exit status 101 and the
exact five failure names; it is not reported as a green suite. The current full
Unix suites also run their installed dynamic-runtime cases, including macOS
dyld-cache loading and Linux installed-library loading. The Linux x64 container
uses translation on an ARM64 host.

Both actual owning routes preserve smoke exit status 0. Rust's `whoami.exe`
terminal index advances from 2832 to 2866, and C ABI turns advance from 2833 to
2867: each difference is 34 turns. Both now stop at `NtQueryInformationProcess`
class 0. The bounded observer records the current-process pseudo-handle
`0xFFFFFFFFFFFFFFFF`, a 48-byte output buffer and a null return-length pointer.
C ABI reason 5 denotes emulator failure despite its zero exit-code field.
The program, source, runtime, selected archives and caller images retain their
recorded before/after hashes. The preceding parent evidence is published under
[Error Message Instrument](../native-error-message-instrument/).

The retained actual owning consumers use Assist ABI 184. The published current
Assist root uses ABI 274. Whole-root plugin, GUI/harness and package results are
tracked through the current-root native CI lanes, separately from these retained
consumer runs. No local whole-root package or independent x64-kernel result is
claimed here.

## Reproduction and build lifecycle

```sh
python3 -B docs/specifications/windows/native-display-policy/check_native_evidence.py --source .
python3 -B docs/specifications/windows/native-display-policy/check_gates.py
```

The public Git parent `39c678a46b069dba9a036524f2e38b096f043dbd` and the 25
individually hashed members in `validation/display-final-compiled-overrides-v1.tar.gz`
reconstruct the frozen compiler envelope. The development materialization archive
is not republished. `validation/mutant-native-cargo-config.toml` supplies the
separately hashed 374-byte native Windows configuration projection. Raw native
observations retain their original bytes.
Six private Assist compiler transcripts have explicit publication projections;
the projection ledger preserves original and published identities. The checkers
pass on macOS, Linux and Windows; `--live-native` additionally rehashes the
recorded Windows artifacts. Embedded Python loads the adjacent verifier by its
explicit path, supporting isolated import configurations.

The evidence stress run rejects eight corrupt cases, including changed statuses,
handles, LastError, view width, duplicate rows, handle counts, attributes and an
archive member with a repaired archive digest. A fresh restored positive passes.

The initial Windows link failed with `LNK1180` before tests. Its receipt remains
separate. Abandoned task builds were inspected and removed; 26 Windows generated
directories represented 40419886811 logical bytes. Windows free space after that
cleanup was 26005729280 bytes. Completed macOS ALPC/ERI build trees were removed.
The Linux standalone and owning caches were byte-verified into their exact source
worktrees; the old owning Docker volume was removed. Subsequent build subprocesses
use in-tree output/temporary paths and disabled external compiler caching.

Current Display and actual owning caches are retained for the next concrete
changeset: implementing and validating the recorded process-information frontier
while preserving these registry regression controls. Unused temporary corruption
fixtures are removed. The original shared Assist worktree and unrelated builds
remain outside this changeset.

| Impact | Remaining finding | Boundary |
| --- | --- | --- |
| High | Both actual callers stop at process-information class 0 | Full-userland goal remains active |
| High | Existing CSR/USER behavior retains its explicit host profile | This root selection does not generalize ALPC |
| Medium | Five native Windows lowerer failures and previously observed Windows x64 CI profile/time assertions remain unresolved | Preserved as failures, with separate root-cause work required |
| Medium | Retained ABI184 consumers differ from current ABI274 shipping products | Current-root CI/package evidence remains separate |
| Medium | Host registry acquisition errors still abort snapshot acquisition | Existing fail-closed capture contract is preserved |

Quality gates: no normative judgment is required; DP-A1--DP-A3 are reconciled;
the fixed-root contract, consumers and platform surfaces have direct evidence;
counts, units and bounds are reproducible; failures and scope limits remain
explicit; primary native records and identities are checked; bounded findings
are recorded. Full-userland acceptance remains open.
