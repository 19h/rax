# Native system time

This group implements `NtQuerySystemTime` for the same-host native Windows
userland. The service contract is implemented and validated; ordinary Windows
application execution remains incomplete at the next measured service.

The selected native ARM64, x64 compatibility, x86 and x86-LAA probes cover 205
calls each, 820 total. They agree on ULONG (4-byte) alignment before memory
access, an atomic 8-byte output, preserved last-error state, and the observed
write-protection/guard ordering. Writable guard pages are consumed; a readonly
guard refuses the write without consuming the guard. A cross-page failure
leaves both halves of the output unchanged. These are observations on Windows
10.0.29683.1000; the x64 compatibility producer is not a physical x64 kernel.

The native service uses the existing real-time clock adapter and the existing
FILETIME conversion used by `GetSystemTimeAsFileTime`. For a Unix sample of
`s` seconds and `n` nanoseconds, the result is
`(s + 11,644,473,600) * 10,000,000 + floor(n / 100)` ticks, each 100 ns, since
1601-01-01T00:00:00Z. This preserves the current host-clock policy; it introduces
no seed-derived clock or guest clock setter. Four-byte alignment is a measured
output-pointer constraint, separate from the eight-byte result width.

[Microsoft's declaration](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntquerysystemtime)
defines the signature, time origin and units. It does not specify all pointer
fault ordering. The native producers, raw transcripts and post-run identities
in `native/` provide those observations. `check-native.py` checks the complete
case axes, raw input hashes, producer identities, clock bounds and caller bytes.
The runtime regression replays every recorded case through all four ABI
profiles and normalizes only successful clock values; it compares status,
protection, last-error and the remaining caller bytes exactly.

The assumption register and bounded-scope records distinguish the service
contract, the short validation clock brackets and unfinished ordinary startup.


## Validation and remaining application boundary

| Host | Full library passed/failed/ignored | Clock focus | C API | Windows integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 7880/0/2 | 2 | 168 | 544; native-memory adapter excluded |
| Linux x86-64 container | 7874/0/2 | 2 | 168 | 544; native-memory adapter excluded |
| Windows ARM64 | 7310/7/2 | 3 | 168 | 4 native-memory cases; Unix corpus excluded |

All-target checks pass on all three hosts. The full Windows run records seven
failures: the unchanged four BZHI and one
FP16 lowerer tests, plus two TLS tests whose heap-commitment assumption was
incorrect. The latter are corrected in the final test-only epoch and pass in
fresh three-host TLS and heap checks; the original failed run is retained.
The two ignored microkernel tests remain excluded. Native Windows registers
zero tests from the Unix-only `user_windows` integration corpus; the 544 Unix
cases are not claimed as Windows executions.

The full suites used the prior image-owner test. The publication tree inherits
that group's later Windows-only portability correction and a TLS test fixture
correction. All 5517 captured Rust inputs agree except these two test files.
Fresh Windows checks pass all three clock tests, all 20 image-extension tests,
all three thread rollback tests, the heap contract tests, and all-target
compilation. Fresh Unix TLS/heap tests and all-target checks also pass. The TLS
tests now warm the allocator cache before retaining the strict commitment
assertion and additionally compare exact live heap blocks and allocation/page
state. Production Rust inputs did not change across this bridge.

Scoped Assist validation passes seven CTests and two protected-command scans on
macOS, and five C++ consumers on Linux and Windows. Each host has two complete
archive member walks for the actual empty-feature core and static carrier,
bound to 354 selected source/fixture inputs. The retained Assist carrier is an
isolated source with `ASSIST_RS_ABI_MINOR=184`; comparison root
`1ee846e278a9c5fa6faff75e6808f88b46c444ea` uses minor191. The nine relevant
C++ inputs remain unchanged. These checks do not claim a whole-current-
root GUI/plugin or package build.

The unchanged production observer now passes `NtQuerySystemTime` and stops 13
instructions later at `NtSetInformationProcess` class 53: smoke at turn97977 and
installed whoami at turn98041. Both terminate with `Complete(Internal(...))`;
the observer's exit code 0 is not a successful application exit. The third
previously named helper executable was absent and is not counted as an execution.
No ordinary Windows application completion or SEH recovery is established.

Native and validation checkers reject six deliberately altered proof sets each,
including rebound digests with changed status, guard lifetime, baseline failures,
source identity, enabled default features and invented application success.
Preparation failures from incomplete source snapshots are retained separately;
the successful Windows run verified the complete 18805-file snapshot before
execution. Those preparation errors required no production-code workaround.
