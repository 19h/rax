# Native single-object wait and alertable completion

This semantic group adds `NtWaitForSingleObject` to the checked native NT
service path. Installed NTDLL supplies the service number and executes its real
return instructions. The operation uses private guest objects and scheduler
parking. It does not wait on a host synchronization handle.

The native reference is Windows 10.0.29683 on one ARM64 installation. ARM64,
x64 compatibility, x86 and large-address-aware x86 producers are separate ABI
profiles on that kernel. These captures do not certify a physical x64 or x86
kernel or another Windows build. All five producer revisions are retained.

## Contract and representations

| Input or state | Behavior |
|---|---|
| `Timeout == NULL` | Infinite; no finite target is invented |
| `*Timeout == 0` | Poll after handle/access validation and eligible APC delivery |
| Negative `LARGE_INTEGER` | Relative interval in 100 ns units, using a clock that excludes host suspend |
| Positive `LARGE_INTEGER` | Absolute NT UTC target, retained across realtime adjustments |
| Unaligned timeout pointer | Accepted; the eight-byte read can cross pages |
| Unreadable or guarded timeout | Read fault/guard result precedes invalid-handle and access checks |
| `Alertable` | Low eight bits determine truth |
| Invalid handle | `STATUS_INVALID_HANDLE`, before pending APC delivery |
| Missing `SYNCHRONIZE` (`0x00100000`) | `STATUS_ACCESS_DENIED`, before pending APC delivery |
| Valid alertable wait with queued ordinary user APC | `STATUS_USER_APC` (`0xC0`), before signal consumption or zero-time expiration |
| Signaled object | Acquire/consume through the existing shared synchronization model, before timeout expiration |
| Abandoned mutant | Transfer ownership and return `0x80` |
| Unmodeled object type | Explicit unsupported operation; no fabricated success |

The admitted existing dispatcher profile comprises Event, Mutex, Semaphore,
Thread, Process, Timer, IrTimer and WaitCompletionPacket. Native and HLE callers
share handle resolution, pseudo-process/thread handles, access validation and
this type policy. Other NT object kinds and native thread alerts are not added
by this group. Termination still uses the existing scheduler lifecycle.

The native producer also calls the three Win32 alertable adapters directly.
On all four captured profiles, `WaitForSingleObjectEx`,
`WaitForMultipleObjectsEx` and `SleepEx` treat `0x100` as false and `0x101` as
true. The adapters now apply that measured low-byte policy and preserve queued
APC priority. `WaitAll` retains full-value BOOL semantics: `0x100` is true.
The global BOOL decoder and unrelated APIs are unchanged.

```text
installed NTDLL kernel entry
  -> capture timeout bytes
  -> validate guest handle/access/type
  -> eligible queued APC / immediate acquisition / expired target
  -> NativeObjects wait + retained object reference + saved native context
       -> signal, deadline or APC
       -> release wait reference
       -> restore native status/context
       -> installed NTDLL return instruction
```

The kernel continuation uses the existing HLE frame storage and `Flow::Resume`
without applying HLE callee cleanup. In WoW64, transition ESP = S, the internal
return is at S, the caller return is at S+4 and argument zero is at S+8. Kernel
completion resumes at S+4. Installed NTDLL then performs its actual `RET 12`,
consuming the caller return plus three four-byte arguments. Native x64 preserves
the SYSCALL return RCX. APC completion retains the interrupted native frame.
Completion, cancellation, normal peer exit and forced process teardown release
the registered object references through the shared lifecycle hooks.

`Wait::NativeObjects` and `NativeDeadline` extend the public Rust source API.
Known Rust consumers must rebuild. The public C ABI, dependency pins, default
features, product permission policy and package layout are unchanged.

## Deadline arithmetic and clocks

For signed timeout ticks T and initial awake time A in ns:

```text
T < 0:  target_awake_ns = A + unsigned_abs(T) * 100 ns
T >= 0: target_UTC_ticks = T
remaining_absolute_ns = max(target_UTC_ticks - current_UTC_ticks, 0) * 100 ns
```

`unsigned_abs(i64::MIN) * 100 = 922337203685477580800 ns` fits `u128`.
Finite large intervals never become infinite through an `Instant` overflow.
Absolute UTC targets remain in their original domain, including backwards
clock changes. The shared checked file-time conversion is
`(Unix_seconds + 11644473600) * 10000000 + nanoseconds / 100` ticks; the existing
system-time query keeps its old alias. Scheduling uses a recheck hint capped
at 10000000 ns (10 ms); actual expiration samples the retained clock domain.
That hint is not a latency guarantee. Host clock failures stop explicitly.

| Host | Relative awake-time source | Binding |
|---|---|---|
| Linux | `CLOCK_MONOTONIC` | `clock_gettime`, normalized seconds/ns |
| macOS | `CLOCK_UPTIME_RAW` | `clock_gettime`, backed by `mach_absolute_time` |
| Windows | `QueryUnbiasedInterruptTimePrecise` | Borrow Kernel32, then KernelBase; dynamic lookup |
| Older Windows fallback | `QueryUnbiasedInterruptTime` | Existing API, 100 ns units with clock-tick quantization |

Microsoft's precise-clock requirements table names Kernel32. Revision 4 found
no export there; revision 5 independently found the export in KernelBase for
all four profiles and bound both DLLs before/after. The older fallback remains
available without adding a static import of the Windows 10 precise API.
Units do not imply clock precision. Physical suspend/resume and host realtime
mutation were not injected; pure deadline tests cover clock movement and domain
independence, while native time cases cover signed values and finite extremes.

## Reference and regression coverage

Each final native profile records 560 timeout/fault cases, 14 native single
APC cases, 14 Win32 single APC cases, 16 signed-time cases, 84 Win32 multiple
APC cases, seven SleepEx APC cases and 84 native object/APC cases. Matrices
include seven alert values, ten timeout-pointer modes, access denial, invalid
handles, guard consumption, read-only/cross-page reads, automatic/manual events,
semaphore consumption, mutant acquisition and already-owned recursion.

The eight exact TSV projections are compiled into the portable core tests:
2240 fault/access/guard rows and 336 object/APC rows. Replays assert status,
state bytes, callback scheduling, frame retention, reference lifetime and stack
behavior. Separate tests cover cancellation, last-handle closure while parked,
teardown, abandoned ownership, deadline ranges and all guest ABI return paths.
The native Windows-only regression executes installed NTDLL wait leaves through
park, wake and their actual return instructions for the host architecture and
installed x86 profile. x64 compatibility reference captures are separate from
that installed-leaf execution coverage.

`check_native.py` verifies producer/source/PE/module identities, exact matrix
coverage, semantic priority and replay projections. It checks 20 retained native
runs. Probe objects are process-private and unnamed, apart from opening the
read-only BaseNamedObjects directory for an access-denied comparison. Sources
and PE files have matching before/after identities, and process handle counts
are stable. Native elapsed times include scheduler/VM jitter and are not
treated as exact requested-time measurements.

The original tests-only baseline had nine failures, including two fixture
cleanup lookups for an API outside that helper's registration families. Both
lookups were corrected to close the private fixture directly. The corrected
baseline compiles and has nine behavioral failures against published parent
`889bc914e298cb58d8ab0b90c9181ef75083f97c`. Original inputs/results remain
retained and explicitly superseded; no claim rests on the two fixture errors.

The final fourth source snapshot contains 22059 tracked/new compilation inputs,
1510895073 bytes before compression. Sixteen symlinks are represented as link
text for portable Windows extraction. Native Cargo changes only offline source
replacement, bound separately. The first macOS and Windows attempts reused
older binaries from shared targets; exact focused counts rejected those runs.
Final retries use fresh targets with the same fixed source snapshot. The
selected retained-consumer manifest has 474 affected inputs and is explicitly
not the complete source closure.

The first Windows retained-consumer helper still hard-coded the parent archive
when Cargo moved to a fresh target. Those C++ results are excluded. A separate
final helper binds the actual Cargo carrier path, byte count and SHA256 before
and after all five C++ consumers. The final fourth-epoch driver collects actual Cargo
archive identities before and after the C++ links. The original script, result
and correction lineage remain retained.

Final platform, carrier, ordinary-program and rejection evidence is recorded in
`validation/`. Those results must be read with their exact build/ABI scope.
The full same-host Linux/macOS/Windows process-emulation objective remains
incomplete; this document does not claim successful ordinary Windows startup.

The ordinal regression was discovered after epoch3 broad gates. Prepending
NtWait renumbered existing synthetic NTDLL entries. A separate test compiles
and fails against that implementation. The fourth epoch retains the entire
primary native table byte-for-byte from the published parent and appends the
new wait table after the existing native and SList families. Installed service
lookup scans both native tables. Actual synthetic PE images preserve every old
ordinal and symbol/trap RVA for all three guest ABIs. The final source snapshot
is derived from the complete epoch3 archive plus three explicit overrides.
Native materialization uses immutable links for unchanged files and separate
changed files. Override mtimes are refreshed after preceding Cargo invocations
finish, and exact16/17-test counts witness the rebuilt final source.

## Assumption register

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
|---|---|---|---|---|---|---|
| W1 | Captured build bounds the measured ABI behavior | Twenty native runs on one ARM64 kernel | Fault order and narrowing policy | Four profiles, pointer boundaries, raw state | Repeat producers on another kernel | Retained; bounded |
| W2 | Wait lifetimes remain private guest references | Shared Objects/scheduler model | Isolation and cleanup | Close last handle, cancel, abandon, exit | Host wait FFI or surviving reference | Tested within admitted profile |
| W3 | Saved native context can reuse frame/Resume machinery | Dispatch contract and ABI tests | Park/APC return | WoW S+4, x64 RCX, same-height APC frame | Wrong PC/SP, double cleanup or frame pruning | Tested; installed leaf scope stated above |
| W4 | Absolute targets retain UTC adjustments; relative targets use awake time | Primary clock contracts, native signed cases, pure domain tests | Finite timeout behavior | i64 extremes and both clock directions | Infinite conversion, overflow or lost adjustment | Retained; physical suspend not injected |
| W5 | Win32 alertable BOOL uses full nonzero truth | Initial candidate contradicted by native captures | Three adapter decoders | Seven raw values on four profiles | 0x100 false, 0x101 true | Falsified; low-byte policy implemented |
| W6 | Kernel32 always provides the documented precise export | Documentation versus v4/v5 capture | Windows clock lookup | Compare two borrowed modules | Kernel32 absent, KernelBase present | Falsified; provider fallback implemented |
| W7 | Source hashes alone establish Cargo artifact freshness across worktrees | Rejected shared-target runs | Build provenance | Fresh targets and exact selection counts | Old nine-test/zero-test artifact | Falsified; final targets isolated |
| W8 | Cargo target selection propagates into a retained C++ helper | First helper hard-coded the parent archive | Native consumer evidence | Actual carrier path/hash/bytes before and after all five links | Parent or changing archive selected | Falsified; separate bound helper used |
| W9 | Prepending a new native entry preserves synthetic ordinals/RVAs | Epoch3 PE regression fails | Existing ordinal imports and trap locations | Compare actual old/new PE images on all guest ABIs | An existing entry moves | Falsified; new table appended after existing families |

## Bounded findings

| Impact | Finding and evidence | Effect |
|---|---|---|
| High | Native service dispatch previously rejected `Flow::Block` | Required continuation repair included |
| High | Shared object polling consumed ready signals before queued APCs; native matrices disagree | Required native/HLE priority correction included |
| High | Broader NT object/file/security/private-BNO and process-startup surfaces remain incomplete | Blocks the overall full-emulation goal |
| Medium | One physical kernel; installed x64 hybrid selection remains a separate runtime surface | Bounds platform/guest claims |
| Medium | Older unbiased clock fallback has clock-tick precision | Explicit compatibility behavior; no precision guarantee |
| Medium | Retained Assist Rust ABI184 differs from the current root Rust port | Exact consumer builds do not establish whole-current-root GUI/plugin/package parity |
| Low | No adjacent ISA/JIT/SMIR optimization or unrelated UI refactor was needed | No adjacent changes made |

## Primary sources

Verified 2026-10-19. Captured user-mode behavior supplies details absent from
these contracts; driver-only APC restrictions are not generalized to the
measured user-mode mutant cases.

- [Microsoft ZwWaitForSingleObject](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-zwwaitforsingleobject): native signature, signed 100 ns times, absolute clock adjustment, user-mode Nt naming and completion statuses.
- [Microsoft WaitForSingleObjectEx](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitforsingleobjectex): access right, completion classes and modern low-power timeout contract.
- [Microsoft WaitForMultipleObjectsEx](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-waitformultipleobjectsex) and [SleepEx](https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-sleepex): adapter contracts; exact noncanonical BOOL behavior is measured here.
- [Microsoft QueryUnbiasedInterruptTimePrecise](https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-queryunbiasedinterrupttimeprecise): awake-time domain, 100 ns units, void/system ABI, dynamic-provider discrepancy retained above.
- [Apple Libc clock_gettime source](https://github.com/apple-oss-distributions/Libc/blob/63976b830a836a22649b806fe62e8614fe3e5555/gen/clock_gettime.c): UPTIME_RAW selects mach_absolute_time; MONOTONIC_RAW selects mach_continuous_time.
- [Linux clock_gettime manual](https://man7.org/linux/man-pages/man2/clock_gettime.2.html): CLOCK_MONOTONIC and suspend-time distinction.


## Final execution evidence

| Host | Full core passed/failed/ignored | Focus | C API | Integration |
|---|---|---|---|---|
| macOS ARM64 | 7961/0/2 | 16 | 168 | 544 |
| Linux x86-64 container | 7955/0/2 | 16 | 168 | 544 |
| Windows ARM64 | 7401/5/2 | 17 | 168 | 4 native-memory; Unix integration binary selects0 |

All-target checks pass on each host. Windows retains exactly the four existing
BZHI lowerer assertions and one FP16 runtime-lowering failure. They are not new
wait failures or newly ignored checks. `check_validation.py` requires their exact
names and the complete unfiltered result counts. The two ignored core checks
remain visible. Retained consumers have two complete actual Cargo archives per
host, empty core features and474 selected affected inputs. The final Windows
C++ links use the exact carrier path/hash/size recorded before and after all five
consumers. These are scoped retained ABI184 carriers. The compared root is
8f49854b/ABI264; publication preserves the independent 04929f5e/ABI265 port,
with the same eight selected C++ sources. Whole-current-root products are outside
this evidence.

Both unchanged ordinary ARM64 programs execute all startup instructions and
return0 from the installed native wait, acquiring private mutant handle0x6C for
guest thread8. The observer executes the initial2100000 turns normally; it does
not skip guest instructions or syscalls. The NULL timeout is infinite, and the
object is already ready. The observer's diagnostic read at address0 is not an
API timeout fault. Final observations are:

| Program | Wait return turn | Next terminal turn | Next missing service |
|---|---|---|---|
| ARM64 smoke fixture | 2151176 | 2151527 | NtOpenSemaphore(0x136), PC0x7FFF38A423A0 |
| Installed whoami.exe | 2153525 | 2153876 | NtOpenSemaphore(0x136), PC0x7FFF38A423A0 |

Each advances351 observer turns beyond the preceding stop. Neither reaches a
successful guest exit. Controller exit0 is separate from guest completion.
Source, executable, programs, actual owner core and six runtime modules have
matching before/after identities. Native-only and final checks reject13 and12
independent corruption controls respectively; final controls rebind outer
hashes before checking semantics. The current complete evidence envelope is
`evidence.json`.
