# Native NtTestAlert compatibility evidence

The zero-argument NTDLL service consumes the guest-owned per-thread user alert
before delivering queued Win32 APCs. A consumed alert returns STATUS_ALERTED
(0x00000101) exactly once and leaves the queue intact. Otherwise the service
returns STATUS_SUCCESS (0), after the existing scheduler drains pending callbacks
in FIFO order, including callbacks appended during delivery and recursive checks.
The captured CPU context is restored; callback writes to guest TEB memory remain
observable. No guest call invokes host NtTestAlert or retains host thread state.

Entry cost is O(1); delivering K callbacks costs O(K) scheduler dispatches plus
their guest instruction execution. Existing queue storage is O(K); nested callback
frames are O(D) in recursion depth D. The x86 installed leaf returns through its
actual zero-argument RET with 4 bytes of return-address cleanup; x64 removes
8 bytes, and ARM64 returns through its link register without stack cleanup.

The public Rust Thread structure adds user_alerted: bool, initialized false in
all constructors. The C ABI remains opaque and unchanged. Existing guest APC
state carries a routine and one argument. NtAlertThread and the native
three-argument APC queue producers remain separate unsupported frontiers; the
full native corpus preserves their observations as controls.

Two independent build/launch paths execute the same formatted C++ source under
ARM64, x64, x86 and x86 large-address-aware profiles on one Windows 29683 ARM64
kernel. The raw records contain 216 top-level checks, 144 callbacks and eight
nested calls; the modeled single-argument projection contains 152 top-level
checks and 80 callbacks. check_native.py independently checks raw identities,
statuses, ordering, native callback arguments, thread identity, stack/nonvolatile
restoration, LastError and the exact projection. The PHNT declarations are
archived with their pinned source and license in primary/.

The compiled tests-only baseline fails both selected tests before the service
exists. Validation records distinguish focused checks, complete library suites,
C API/integration compilation, retained linked consumers and ordinary guests.
Successful ordinary guest exit and the full process objective remain unproved
until recorded by actual owning execution.

Assumptions, falsification probes, change surfaces, bounded findings and final
acceptance evidence are recorded in validation/.

The installed WoW64 service has full encoded identity 0x000201d3 and ends in
plain RET. The decoder admits this complete direct zero-argument leaf alongside
its existing RET imm16 form, still validating the exported transition thunk and
preserving every encoded number bit. Actual installed bytes, DLL identities,
transition target and compiled decoder regression are retained as evidence.

## Pre-integration validation (source epoch3)

| Host | Library passed | Failed | Ignored | C API passed | Integration passed | All targets |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| macOS | 8057 | 0 | 2 | 183 | 544 | pass |
| Linux | 8051 | 0 | 2 | 183 | 544 | pass |
| Windows | 7521 | 5 established lowerer failures | 2 | 183 | 4 | pass |

Eight shared focused tests and nine Windows tests pass. The Windows selection
executes installed native/WoW64 leaves, actual CPU callbacks and x86 RET cleanup.
Both compiled regressions are preserved: service absence (0 pass /2 fail), and
the plain-RET decoder (0 pass /1 fail). Fourteen rebound native corruptions are
rejected. Complete immutable source epoch3: 25088 files, 1830323541 bytes.

Retained Assist ABI184 consumers pass seven macOS CTests and five C++ consumers
on Linux and Windows, with 884 selected source inputs, full archive walks and
unchanged before/after archive identities. Six actual macOS linker commands
select the current carrier. These are separate from whole current-root ABI264
GUI/plugin/package validation. Controller staging/fixture/metadata errors and
their corrections are retained; final results are bound to actual source and
Cargo artifact paths rather than inherited controller labels.

The actual owning Windows core is 507091576 bytes, SHA-256
`d4606aa7733aef0aea679b473ce9d22ccaecf30862908ce441b31d17cca9002c`.
Smoke calls NtTestAlert at observer turn2943177, receives status0 and continues
to turn2943204: 2943204-2943177=27 additional observer turns. It then stops at
explicit unsupported NtContinue alertable APC delivery. Whoami remains at
NtQueryVirtualMemory class3, turn2966197. Both native controls exit0; source,
core, observer, executable, program and six DLL identities remain unchanged
through the corresponding checks. Neither guest exits successfully; the full
process objective remains active.

See validation/final-assumptions.json for the complete Assumption Register,
validation/bounded-findings.json for remaining scope and validation/summary.json
for the exact acceptance ledger. Run `python3 -B check_validation.py --host HOST`
from this directory, where HOST is macos, linux or windows. Native materializers
retain the single recorded Cargo configuration adaptation.

Seventeen final receipt/record corruptions are rejected after envelope and
applicable log/record hashes are rebound. The portable checker accepts either
the original repository Cargo configuration or the exact recorded 374-byte
native Windows offline-registry adaptation; other source drift is rejected.

Portable proof verification completed on macOS, Linux and native Windows. The
first checked envelope and its three receipts are preserved in validation/.
Final publication verifies the expanded envelope containing those receipts.

## Current integrated source validation

Concurrent master advanced to `1b25f3438f63d8cc256ae05ca4270e675a084163`.
The linear integration preserves its 111 changed paths and the byte-identical
nine feature-source paths. `validation/integration-epoch4/overlay.json` records
all before/after identities. Epoch3 above remains historical proof; the current
source was rebuilt and exercised separately:

| Host | Library passed | Failed | Ignored | C API passed | Integration passed | All targets |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| macOS | 8070 | 0 | 2 | 183 | 544 | pass |
| Linux | 8064 | 0 | 2 | 183 | 544 | pass |
| Windows | 7530 | 5 established lowerer failures | 2 | 183 | 4 | pass |

The current compilation graph contains 25066 files and 1834179374 bytes.
Derived feature evidence is excluded except the compiled modeled-case dataset.
The owning closure contains 3808 inputs. Seven macOS CTests, five Linux and five
Windows C++ consumers, full archive member walks and six macOS linker commands
pass; corresponding archive identities remain stable before and after checks.
These remain retained ABI184 consumers, distinct from whole current-root ABI264
GUI/plugin/package validation.

Current actual owning core: 508200048 bytes, SHA-256
`bc6d2a0a6e1f59d75f3db0bad382db820723b162dce8ebb5dcc331e40acbb120`.
Smoke again receives NtTestAlert status0 at turn2943177 and advances27observer
turns to NtContinue at2943204. Its supplied continuation is valid, with flags
0x400017, PC0x7fff38b24d30 and SP0x93ffc0. Whoami remains at memory class3,
turn2966197. Both native controls exit0. Current source, core, observer,
executable, program and six runtime-DLL identities remain stable. Neither
emulated guest exits successfully; the full process objective remains active.

The first current ordinary controller completed its build, then stopped because
proof-only native metadata was absent from the frozen compilation graph. The
corrected controller verifies and reuses that exact built executable/core/source
and reads the unchanged pinned native metadata separately. Both raw receipts are
preserved. Current source checks run in addition to the complete historical
checker; they do not substitute a source overlay for fresh executable evidence.

Current integration assumption A5, Quality Gates and bounded findings are in
`validation/integration-epoch4/`. Publication uses the final proof envelope and
independent portable verification on all three operating systems.

Ten fresh integration corruptions are rejected with receipt/log hashes and
coherent counts rebound where applicable. The independent current-source
checker validates exact current counts, source, owning archives and ordinary
frontiers in addition to the historical native and final negative controls.

Current integrated proof verification passes on macOS, Linux and native Windows.
The first checked envelope and its three raw receipts/logs are preserved under
`validation/integration-epoch4/`; publication verifies the expanded final envelope.
