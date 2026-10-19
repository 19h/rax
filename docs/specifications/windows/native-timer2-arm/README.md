# Native extended timer arming and cancellation

This semantic group implements `NtSetTimer2` and `NtCancelTimer2` for the
extended objects created by `NtCreateTimer2`. It includes expiration through the
existing scheduler, native/Win32 object waits, and wait-completion packets.
The overall same-host Linux/macOS/Windows process objective remains active;
the ordinary-program observations below define the demonstrated Windows scope.

## Native contract and independent evidence

The four native producers ran ARM64, compatibility x64, x86, and x86 LAA
profiles on one Windows ARM64 kernel, build 29683. These are four ABI profiles,
not four independent kernels. Before/after PE, producer-source, and six runtime
module identities are retained. Every producer exits zero, and private handle
counts balance. The selected System32 NTDLL SHA-256 is
`502d4678206c8fd564972a3f82bfe63fe1a451cb5a6804ac0a9823b65fcbb61f`.

| Capture family | Per profile | Total | Purpose |
| --- | ---: | ---: | --- |
| `native-v1/` | 842 | 3,368 | Signed due/period domains, parameter values, access, types, state |
| `native-fault-v2/` | 3,806 | 15,224 | Input span/alignment, guard consumption, read order, ignored cancel pointer |
| `native-state-v3/` | 13 | 52 | Bounded periodic and rejected-rearm observations |
| `native-period-v4/` | 24 | 96 | QPC timing distinguishes negative periods from large unsigned delays |
| Total | 4,685 | 18,740 | All producer outcomes and identities checked |

The portable replay executes 18,528 exact value/access/status/guard records.
The remaining 212 records involve asynchronous wall-clock state/timing; their
capture, identities, sequence, and applicable invariants are checked without
making sampled poll races exact golden values. Scheduling and state behavior
also have direct lifecycle tests. Fourteen independently corrupted capture
copies are rejected by `check-native-timer2.py`; original captures are unchanged.
Thirteen validation-corruption controls also fail after their altered log
digests and result metadata are rebound, rather than relying on an outer hash.

| Input or operation | Native result represented by the implementation |
| --- | --- |
| `DueTime` | Required 8-byte signed value; NULL returns `0xC00000F0` |
| `Period` | Optional 8-byte signed value; NULL or zero is one-shot |
| Nonzero period, IR role bit `0x2` | `0xC000000D` |
| Other extended timer roles | Entire signed 64-bit period domain is accepted |
| Parameters | Optional 16-byte `ULONG Version`, `ULONG Reserved`, `LONGLONG NoWakeTolerance` |
| Nonzero `Version` or tolerance below -1 | `0xC00000F1` |
| `Reserved` | Ignored by value, included in the full memory capture |
| Modify access | `TIMER_MODIFY_STATE`, bit `0x2`, required after input capture |
| Legacy timer, event, mutant, pseudo-handle | `0xC0000024` |
| Invalid handle | `0xC0000008` |
| `NtCancelTimer2` second argument | Never dereferenced; no guard consumption |
| Successful rearm | Replaces pending operation and resets signal state |
| Failed rearm | Preserves signal and pending operation |
| Successful cancel | Removes pending operation and preserves signal state |

Native64 performs NULL-due validation, due capture, optional period capture,
then optional parameter capture. WoW64 captures parameters first, before
NULL-due validation and due/period capture. Both then perform typed-handle and
access checks before parameter-value validation. Unaligned inputs are accepted.
The fault replay independently checks statuses and which guards are consumed.

The pinned PHNT header and license are stored here with their retrieval record.
The declaration comes from commit
`53fbbdc5b5d2b08761db1c7b26bfa8c820924356`; the header SHA-256 is
`0510ac40fa1690cd73bed8afe3ed602aaa7bb6870ab95ea9aaba36ce1f4c3d53`.
Stored Microsoft documents describe clock domains and dispatcher mechanics:
[ExSetTimer](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-exsettimer)
and [SetWaitableTimerEx](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-setwaitabletimerex).
Those separate APIs do not define this native call's validation restrictions:
native captures establish acceptance of large/negative periods and absolute
due times, including high-resolution attribute cases.

## Time model, state, and complexity

Each native tick is 100 ns. The observed due value is
`-3,000,000,000 ticks * 100 ns/tick = -300 s`; its relative delay is 300 s.
The tolerance is `750,000,000 ticks * 100 ns/tick = 75 s`.
The maximum relative delay is
`2^63 ticks * 100 ns/tick = 922,337,203,685,477,580,800 ns`.
Widened arithmetic retains that value; it is never converted into an
overflowing host `Instant` or truncated millisecond count.

Relative targets use the existing suspend-excluding native deadline adapter.
Absolute targets retain UTC ticks since 1601 and are resampled rather than
permanently translated to monotonic time. Periodic expiration after the initial
absolute expiration uses the relative clock. Positive periods preserve phase
and coalesce missed periods in O(1) time and O(1) storage. Negative periods
remain fast repeating, with at most one delivered expiration per scheduler
turn; they are not interpreted as unsigned long delays or absolute magnitudes.
Native timing captures have load-dependent jitter. Exact native clock quantum,
hardware resolution, power wakeup, and real-time timing guarantees are outside
the demonstrated timing precision. Tolerance permits coalescing; the model
chooses zero additional delay within that allowance.

Scheduler timer scanning is O(number of kernel objects) per turn. Existing
notification/synchronization signal consumption and wait pinning are reused.
Clock sampling errors stop with an explicit internal diagnostic. Legacy timer
fields and behavior remain intact. New NTDLL exports are appended; every prior
ordinal and trap RVA is checked. The public C ABI remains 1.14.0.

## Assumption register and affected surfaces

The initial plan is preserved in `plan-and-ownership.json`; the final register
is in `validation/final-assumptions.json`, including bases, dependent results,
stress tests, and falsification probes. A1 (existing extended object identity),
A2 (16-byte parameter layout and capture), and A3 (private producer scope) are
confirmed by the native records and lifetime checks. A4 retains the inference
that native relative Timer2 uses the modern suspend-excluding timer clock;
the adapter and clock-domain tests run on all three hosts. A physical
NtSetTimer2 suspend/resume experiment is unknown and is not claimed.

Affected planes are Windows objects, native/HLE service routing, appended
synthetic PE exports, deadline reuse, scheduler expiry, waits, and completion
packets. Existing ISA decoders/executors, SMIR, JIT, VM backends, public C ABI,
dependency/default settings, and packages are unchanged. The exact owned source
list is in `source-ownership.json`; the complete pre-gate source manifest binds
23,050 inputs and 1,599,183,887 bytes. Native Cargo's separate 374-byte source
adapter is explicitly identified. Engineering documentation appended after
compiled gates has its own before/after and prefix identity record.

## Validation and ordinary programs

Five strategic tests compile and fail through the real dispatcher before
implementation because `NtSetTimer2` is missing. The first implementation
batch reports 13/1; its one failure incorrectly expected a completed wait
packet to retain its target. The corrected assertion requires destruction
after public handles and the queued-notification target pin are released.
Production code is unchanged by that correction. The subsequent 14 shared
tests pass, including all 18,528 replay records. Windows adds execution of
installed NTDLL host/WoW64 timer leaves and checks exact return cleanup.

| Host | Unfiltered core: passed / failed / ignored | New focus | C API | Process integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 7,990 / 0 / 2 | 14 passed | 183 passed | 544 passed + 0 cfg cases |
| Linux x86-64 container | 7,984 / 0 / 2 | 14 passed | 183 passed | 544 passed + 0 cfg cases |
| Windows ARM64 | 7,434 / 5 / 2 | 15 passed | 183 passed | 0 cfg cases + 4 native passed |

All three all-target checks pass. Complete core selections are unfiltered.
The exact five retained Windows lowerer failures are recorded in
`validation/windows-existing-failures.json`.

Actual retained Assist core and carrier archives are walked and hashed before
and after C++ consumption on all hosts. The 519 selected source inputs match.
Seven named macOS CTests, actual CLI/MCP linkage and protected-static-text
checks, and five Linux/Windows C++ consumers pass. These retain Assist Rust
ABI184; eight C++ inputs were compared with root ABI264. They do not establish
whole-current-root GUI/plugin/package builds. The Linux x86-64 container runs
under host translation and is distinct from a physical x86-64 kernel.

The smoke program passes the former timer stop at turn 2,188,223 and
reaches turn 2,883,900. `whoami` passes turn 2,190,508 and reaches turn
2,886,185. Both execute another 695,677 observer turns and stop at an uncaptured
`\Registry\MACHINE\System\CurrentControlSet\Control\SafeBoot\Option`
key. Timer arming returns success at the former frontier. Neither guest exits
successfully. Source, programs, observer executable, core archive, and six
runtime modules are stable before/after. The selected Windows core is
503,289,084 bytes with SHA-256 `082390d8d0aca0c04aa96dc484b1111c6a3bb756ae39faad4ca75c4b4a227e76`.

The reproducible commands, source identities, logs, archives, checker, and
negative controls are under `validation/`. Controller completion is recorded
separately from each guest's terminal result. All prefix turns execute normally;
the diagnostic changes no process state or service dispatch.

## Bounded findings and quality gates

| Impact | Finding | Effect on scope |
| --- | --- | --- |
| High | The full userland objective requires successful ordinary runtime paths across supported systems | Remains a separate active acceptance gate |
| Medium | Four native ABI profiles share one ARM64 Windows kernel | Limits independent-kernel claims |
| Medium | Five established Windows lowerer assertions remain failures | Blocks an all-tests-passed claim |
| Medium | Retained Assist consumers differ from whole-current-root products | Limits shipping-product build claims |
| Medium | Native timer quantum, hardware resolution, and physical suspend/resume were not equivalence-tested | Limits timing/power precision claims |

`validation/quality-gates.json` audits QG1-QG7 for this semantic group. No
normative judgment is needed. Units, input widths, status precedence, guard
side effects, object lifetime, source/gate selection, and provenance are checked.
The complete process objective is not declared complete by this record.
