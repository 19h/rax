# Installed ProcessBasicInformation

`NtQueryInformationProcess` class 0 describes the existing virtual self-process.
The five arguments are a process handle, information class, output address,
output length in bytes and an optional `ULONG` return-length address. The
[Microsoft contract](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntqueryinformationprocess)
documents the six basic fields. Exact extended layouts, reserved bytes and
fault ordering below come from the independent native producer in [native](native).
The source was accessed on 2026-10-22; its general short-buffer description does
not establish this class's observed unchanged return-length destination.

## Layout and identity

| Field | Native 64-bit offset/size, bytes | WoW64 offset/size, bytes | Virtual value |
| --- | --- | --- | --- |
| ExitStatus | 0/8, high half zero | 0/4 | Existing exit code or `STATUS_PENDING` (`0x103`) |
| PebBaseAddress | 8/8 | 4/4 | Existing guest PEB |
| AffinityMask | 16/8 | 8/4 | 1, matching the one-CPU guest topology |
| BasePriority/reserved word | 24/8 | 12/4, untouched | 8 on native 64-bit |
| UniqueProcessId | 32/8 | 16/4 | Existing guest PID |
| InheritedFromUniqueProcessId | 40/8 | 20/4 | 0, the existing standalone root-parent contract |

Basic records occupy 48 bytes on ARM64/x64 and 24 bytes on WoW64. Exact extended
sizes are 64 and 32 bytes respectively. Extended records prefix one `SIZE_T`
size, followed by the basic record and flags. Native 64-bit flags/padding are
zero; successful WoW64 conversion sets flags to 2. The input size word is ignored
and replaced. Other tested lengths fail without publishing a required length.
No guest pointer or handle is forwarded to a host process query.

Native pseudo-handles and granted real self-process handles use the existing
object table. Class 0 requires `PROCESS_QUERY_LIMITED_INFORMATION` (`0x1000`);
native duplication of a requested `0x400` grant produces an independently
measured actual grant of `0x1400`. Tests supply that measured actual grant rather
than changing the object table's permission policy. Wrong-type and invalid
handles preserve their distinct statuses. A foreign modeled process remains
an explicit unsupported boundary.

Each call performs a bounded number of existing object/VM lookups and stores,
with O(1) additional work and storage apart from those index operations. The
largest successful record is 64 bytes. Native extended copying performs at
most nine stores including optional return length; WoW64 extended conversion
performs at most eight. There is no length-dependent output allocation.

## Observed ordering

```text
native 64-bit: output alignment/range -> ReturnLength probe -> exact length
              -> handle/type/grant -> sequential 8-byte stores -> ReturnLength

WoW64: exact length -> ReturnLength probe -> native result / warning conversion
       -> ReturnLength -> output conversion stores
```

Native 64-bit entries require 4-byte output alignment when length is nonzero.
They check the caller span but do not pre-probe every output page. An 8-byte
store crossing an inaccessible page leaves earlier stores intact. `ReturnLength`
is published only after successful output copying.

WoW64 leaves the priority word untouched. Basic conversion writes ExitStatus,
AffinityMask, parent PID, PID and PEB in that observed order. Extended conversion
validates the complete destination before writing it. Return-length aliasing
therefore differs from native 64-bit copying. A return-length guard warning can
convert a zeroed native record while a separate successful self-process PEB
lookup still supplies the WoW PEB. Extended flags remain zero on that warning
path. Later output faults override the warning. Direct stores to
`READONLY|GUARD` preserve the guard and report access violation; reached writable
guards are consumed. Native probe guards retain their separately observed order.

## Native evidence and executable checks

Producer v5 records 163 calls in each of four execution views, 652 total. Producer
v6 adds 2400 fault cases, each initialized with both `0xA5` and `0x5A`:
`163 + 2400 * 2 = 4963` calls per view and `4963 * 4 = 19852` calls. Both captures
together contain `652 + 19852 = 20504` calls. The views are ARM64, x64, x86 WoW64
and large-address-aware x86 WoW64 on one ARM64 kernel. They do not establish
independent physical x64-kernel coverage.

The final audit repeats all 19852 v6 calls with unchanged images. This brings
native observations to `20504 + 19852 = 40356`. The repeat compares status order,
return lengths and protection transitions; host-dependent payload identities
are checked through the separately normalized scheduler replay. Probe source,
runtime and caller identities retain their recorded hashes.

Fault cases vary handles, lengths, alignment, null/upper/cross-page addresses,
return-length aliases and read-only/inaccessible/guarded protections. Each uses
two separate three-page arenas with 4096-byte pages and inaccessible outer
pages. The producer checks the actual page size before using these offsets.
Snapshots preserve 128 bytes at each output-page edge and 32/16 bytes at the
return-page edges. Protection values are recorded before restoring access for
inspection. Each native call preserves `LastError=0x12345678`; no escaped
structured exception occurs. Own duplicated/event handles are closed and
before/after process handle counts agree.

The scheduler replay consumes all 19852 v6 rows across the three guest ABIs,
including both x86 address-awareness captures. It compares status, memory bytes,
return length, protection transitions, LastError and dispatch lifetime. Expected
bytes come from the native records. Only recorded host PID, PEB, affinity and
parent fields are projected onto the virtual process; return-length bytes that
overwrite a native field remain exact. Replay work is O(N + B), where N is the
number of observations and B the total parsed/snapshotted bytes. Arenas are
reused within a view; retained fixture text is O(B).

```sh
python3 -B docs/specifications/windows/native-process-basic-information/check_native_evidence.py
python3 -B docs/specifications/windows/native-process-basic-information/check_gates.py --source .
cargo +1.95.0 test --locked --offline --no-default-features --lib native_process_basic_information -- --nocapture --test-threads=1
```

The Python checker verifies the complete fixture catalog, source/image/runtime
before/after identities, compiler/run results, exact row counts, unique two-pattern
fault pairs, snapshot sizes and balanced handles. `--live-native` additionally
rehashes the recorded Windows files. Producer v1 was not executed; v2--v4 were
unsuccessful preparation attempts and are not accepted oracle captures.

## Assumption register

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| PBI-A1 | Query fields describe the existing virtual process | `Proc.pid/peb/exit_code`, one-CPU topology and root-parent contract | Identity projection | All guest ABIs, pseudo/real/tagged handles and alias/fault matrix | Disagreement with another guest identity/topology query | Confirmed for captured fields and replayed boundaries |
| PBI-A2 | Four views cover one ARM64 kernel | Native compiler/run and image records | Platform evidence boundary | ARM64/x64/x86/LAA native captures | An independently validated x64 kernel supplies additional evidence | Confirmed |
| PBI-A3 | Selected compiler inputs and owning archives correspond to the implementation | Frozen 29139-input graph, 3972-input owning envelope and compiler-selected archives | Attribution of validation/caller results | Source hashes, archive walks, before/after identities and corrupt-record controls | A source or selected archive mismatch disproves attribution | Confirmed for recorded envelopes |

## Change surface and bounded findings

| Plane | Effect or reason unaffected |
| --- | --- |
| NT dispatch and guest memory | Class 0 routes to a dedicated shared implementation before the older generic query probes |
| Object permissions/lifetime | Existing table, normalization and grants; no permission-policy change or persistent allocation |
| Process identity/topology | Uses existing virtual fields; no host identity or affinity capture |
| Windows/macOS/Linux targets | Shared Rust implementation and scheduler tests; installed Windows execution requires separate native runtime validation |
| Plugin lifecycle, UI, conversation, providers, prompt/context, tool schemas, main-thread dispatch, mesh/transport and persistence | No contract/source change; only the embedded engine's process query changes |
| SDK/C ABI, optional engines, dependency pins, resource discovery, updates and package layout | No layout, symbol, dependency, feature default or staging change |
| Tests/provenance | New identity regression, captured-call replay, native producer records and independent checker |

| Impact | Finding | Blocking boundary |
| --- | --- | --- |
| High | Foreign-process class 0 remains explicitly unsupported | Broader full-userland goal remains open |
| High | Existing CSR/USER behavior retains its explicit host profile | This process query does not generalize ALPC |
| High | Both actual owning callers now stop at `NtReadVirtualMemory` (`0x3F`) | Broader full-userland execution remains incomplete |
| High | Published-parent Linux CI opens an exited guest thread's `/proc/<tid>/status` instead of returning `ENOENT` | Cause unknown; separately recorded collision/fallback falsification probe; no Linux source change here |
| Medium | Native Windows full suite exposes an unchanged WNF test's hardcoded state/stamp1 while the native capture returns state/stamp4 | All four WNF test/model/capture files match the published parent; the changing actor is unknown; separate repair remains required |
| Medium | Independent x64-kernel behavior is not established by these captures | Additional native platform proof remains separate |
| Medium | Retained ABI184 consumers differ from current Assist ABI274 products | Owning-call and whole-current-root CI/package evidence must remain distinct |

The actual owning Rust caller advances from terminal index 2866 to 2867; C ABI
turns advance from 2867 to 2868. Each difference is one turn. Both retain smoke
exit status 0 and stop at `NtReadVirtualMemory`. Its recorded ARM64 arguments
are self handle `0xFFFFFFFFFFFFFFFF`, base `0x7FFDF010`, output `0x1A5DBA8`,
length 8 bytes and returned-length address `0x1A5DBA0`. The source bytes are
`00 00 00 40 01 00 00 00`, the existing guest image-base value `0x140000000`.
C ABI reason5 remains an emulator failure despite its zero exit-code field.

## Validation and build substrate

| Host | Unfiltered library | C API/integration | Actual retained Assist consumers |
| --- | --- | --- | --- |
| macOS ARM64 | 8178 passed, zero failed, two ignored | 183 / 544 passed | Eight requested targets built; seven CTests passed |
| Linux x64 container | 8172 passed, zero failed, two ignored | 183 / 544 passed | Five C++ consumers passed |
| Windows ARM64 | 7649 passed, six failed, two ignored | 183 / four passed; companion native-cfg binary registers zero tests | Five C++ consumers passed |

The six native Windows failures comprise the five unchanged lowerer failures
plus the unchanged WNF state/stamp assertion described above. Full-suite exit
status 101 is preserved. The continuation imports that exact receipt and runs
the remaining gates; it does not turn the full suite green. All process-query
focused tests and captured-call replays pass on each host. Rustfmt 1.95 and
the all-target Clippy check with `x86_64-suite` pass. All-target checks pass on
Windows, macOS and Linux. The Windows full-suite failures remain explicit.

The first Linux full run used temporary files on the macOS bind mount and
reported 17 failures involving xattrs, socket metadata, case sensitivity and
birth timestamps. All 17 subsequently executed individually and passed with
the same test image on native Linux temporary storage. The complete retry is
unfiltered and passes. Its temporary mount resolves inside the exact Linux
worktree at `/work/vendor/rax/target/linux/tmp`; `/proc/self/mountinfo` identifies
the filesystem as Btrfs. The Cargo target remains the reused in-worktree
`target/linux/candidate1`. The temporary volume is task-owned and is removed
after its active validation use. No assertion, source behavior or test selection
was weakened for this substrate correction.

The frozen graph binds 29139 inputs, including 16 symlink-text inputs. The
29132-file published parent plus ten separately hashed overlay members
reconstructs it. Overlay members comprise six Rust files and four v6 captures;
three of the Rust files are new, giving `29132 + 3 + 4 = 29139` inputs. Actual
owning envelopes bind 3972 RAX inputs. Native Windows uses its separately hashed
374-byte Cargo configuration projection. Selected core/carrier archives retain
their identities across consumer runs. The public C API header remains 70196
bytes with SHA-256 `e8438269c7f9411da7c92dff70942dbb839570cda9a6ac1bec0b78ed731e829a`.

The native checker rejects seven corrupted data cases, then accepts a restored
positive. It also rejects optimized Python (`-O` and `-OO`) so assertions cannot
silently disappear. Raw captured bytes are preserved through scoped Git
attributes. Private Assist compiler diagnostics require explicit publication
projections; original and distributed identities remain distinct.

This record does not claim full-userland completion. The retained actual
consumers use Assist ABI184; current-root ABI274 shipping products and package
results remain separately attributable to the new-pin native CI lanes.

Both executable verifiers pass on macOS, Linux and Windows, including complete
compiler-frame verification. Windows additionally passes the live native hash
check. The task-owned Linux temporary volume was inspected for active users
and removed after validation. Existing in-worktree standalone and actual owning
caches are retained for the concrete WNF/procfs corrections and subsequent
`NtReadVirtualMemory` implementation. Native probe images remain regression
controls; the original shared worktree and unrelated builds are preserved.

Quality gates: no normative judgment is required; PBI-A1--PBI-A3 are reconciled;
the source/consumer/platform map and current-query acceptance have direct
evidence; sizes, offsets and counts are reproducible; full-suite failures and
unavailable broader behavior remain explicit; primary observations and identities
are executable-checked; bounded follow-up findings are recorded. The complete
Linux/macOS/Windows userland goal remains active.
