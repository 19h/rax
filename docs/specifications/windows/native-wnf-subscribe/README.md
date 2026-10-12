# Native process-scoped WNF subscriptions

Measured host: Windows ARM64 10.0.29683.1000. Native callers include ARM64, x64
translation, WoW64 x86 and WoW64 x86 with `IMAGE_FILE_LARGE_ADDRESS_AWARE`.
Physical Windows x64 execution is unknown. This increment implements local
subscriptions/unsubscriptions for the measured provisioned process scope. It
does not establish complete application execution or WNF data/delivery support.

## Implemented contract

`NtSubscribeWnfStateChange` accepts an 8 B name pointer, ULONG change stamp,
ULONG event mask and optional 8 B identifier output. `NtUnsubscribeWnfStateChange`
accepts one 8 B name pointer. All profiles accept byte alignment. Masks use only
bits 0 through 4. The installed kernel validates the name encoding after capture;
version must be 1, scope must be 0 through 5, and persistence is admitted only
for scopes 0, 2 and 4. Syntactically valid absent names return `C0000034`.

| Caller | Output access before name capture | Capture/copy order | Failed final copy |
|---|---|---|---|
| ARM64/x64 | Entire 8 B writable span | Probe -> clear 8 B -> capture name -> admit/publish -> write ID | Rejected before publication by the initial probe |
| WoW64 x86/x86-LAA | First 4 B readable span | Read DWORD -> capture name -> private output/admit/publish -> full 8 B copy | Can override input status or follow publication; all 8 B remain unchanged in measured cases |

The 840 native guard cases distinguish 1 B, 3 B and 4 B readable prefixes before
read-only, guard and inaccessible second pages. Guard faults consume the first
touched guard. WoW64 consumes an input guard before a later copy fault overrides
its status. Output/input aliasing therefore differs between native64 and WoW64.
Opaque native ID numbers are not copied into expected guest state; their nonzero
generation, equality on update and relative byte footprint are checked.

Installed-runtime selection reads the fixed key
`\Registry\Machine\System\CurrentControlSet\Control\Notifications` and captures
the creator process primary token's user/groups/restricted-SID/AppContainer
information with `TOKEN_QUERY`. It closes all acquisition handles and retains
copied metadata. Guest calls never acquire HKEYs, host tokens or host WNF state.
This subject does not replace guest token APIs or the configured guest user name.

The retained catalog has 1613 binary values. Each contains a self-relative
security descriptor followed by a maximum-state-size DWORD. Nine values have
process scope. Admission is bounded to classic noninherited allow/deny ACEs,
exact SID encoding and the measured grants 0, 1 and 3. Other scopes/descriptors,
matched generic rights, restricted tokens, AppContainers and an unmeasured
standalone grant 2 produce an explicit unsupported diagnostic.

| Process state name | Maximum bytes | Captured SYSTEM grants | Admitted masks |
|---|---:|---:|---|
| `02821B2CA3BC08B5` | 16 | 1 | 0, 1, 16, 17 |
| `0D920D2EA3BC08B5` | 500 | 0 | 0 |
| `41911A28A3BC10B5` | 0 | 3 | 0 through 31 |
| `41960A28A3BC50B5` | 4 | 3 | 0 through 31 |
| `41C61629A3BC30B5` | 24 | 1 | 0, 1, 16, 17 |
| `41C61629A3BC60B5` | 4 | 1 | 0, 1, 16, 17 |
| `41C61629A3BCA8B5` | 4 | 0 | 0 |
| `41C61629A3BD10B5` | 24 | 1 | 0, 1, 16, 17 |
| `41C61D3DA3BC10B5` | 0 | 3 | 0 through 31 |

Mask 0 requires no captured grant; masks containing bits 1, 2 or 3 require
grants 3; other nonzero admitted masks require grant 1. Three temporary-state
creation trials failed before subscription. They do not establish grant-2
behavior or prove creation is generally unavailable.

Process-owned subscriptions retain a private generation, requested stamp/mask
and pending metadata. Update preserves ID and replaces the request. Unsubscribe
removes the record; repeat returns `C0000034`. Catalog selection and generation
history survive object drain, while subscriptions and the Event pin are cleared.
No subscription creates a guest handle.

For the measured empty initial state, with previous mask `o` and new mask `m`:

```text
L(m) = 8 | (2 if m&1 else 4 if m&0xE else 0)
arrived = m & L(m) & (~o | ~L(o))
pending = (previous_pending & m) | arrived

subscribe -> local record/pending -> existing Event signal
register Event after subscribe -> existing pending, no retroactive signal
unsubscribe -> discard record/pending -> Event signal remains
```

The first subscription uses previous mask/levels 0. An arriving metadata bit
signals an already registered Event. Manual reset does not erase the pending
record. Unchanged masks and changed requested stamps do not invent new arrival.
Delivery/acknowledgement is still unsupported: successful native delivery can be
followed by repeated metadata after a subscription update. These observations
do not justify a simplified permanent-consumption model.

Catalog selection costs `O(V log V + A*S)` time and `O(V)` retained records,
where `V <= 4096`, `A` is total ACE count and `S` is captured SID count. Local
subscription/update/unsubscribe costs `O(log N)` time and `O(N)` retained
records, bounded by selected names. Fixed capture/output uses `O(1)` space.

## Native corpus and validation

Counts are complete probe cases, not instruction counts or total syscall calls.
The five debugger warmups are retained and excluded from the 30,332 cases.

| Corpus | Profiles | Cases |
|---|---:|---:|
| Basic names/stamps/masks | 4 | 360 |
| Single-bit names and masks | 4 | 17,152 |
| Pointer status and output spans | 4 | 2,496 |
| Mask updates and identity | 4 | 256 |
| Syntax domain | 4 | 8,192 |
| Input/output aliasing | 4 | 228 |
| Guard/read-only/crossing output | 4 | 840 |
| Fresh registration order | ARM64 | 64 |
| Fresh reset/update effects | ARM64 | 128 |
| Native delivery observations | ARM64 | 128 |
| Pending metadata before delivery | ARM64 | 128 |
| Mature process-name permissions | ARM64 | 72 |
| Fresh nine-name masks/metadata | ARM64 | 288 |
| Total excluding warmups | | 30,332 |

Fresh producers use owned debugger children and verified installed NTDLL
entry/return sites before RTL's first notification Event registration. They
restore patched instructions, close aliases and terminate those children with
exit 77. Update probes intentionally reset the owned notification Event. The
first 128 B delivery buffer returned `C0000023`; the corrected 8192 B buffer
captures a 48 B header or `8000001A`. No minimum successful size is established.
Compatibility fresh-state Event behavior remains an explicit extrapolation.

| Host | Focused passed | Full passed/failed/ignored | C API passed | Integration passed | Owning C++ passed |
|---|---:|---|---:|---:|---:|
| macOS | 14 | 7674/0/2 | 168 | 544 | 5 |
| Linux | 14 | 7666/2/2 | 168 | 544 | 5 |
| Windows ARM64 | 15 | 7078/5/2 | 168 | 4 | 5 |

Sixteen selections per host include the prior WNF Event group, earlier native
service regressions, all targets, C API and integrations. Linux CPU execution
uses amd64 translation with a native Linux kernel. Its two prior `io_uring`
timeout failures pass in exact isolated runs. Windows retains four prior BZHI
and one FP16 lowering failure. No assertion or platform skip was weakened.
Windows focused tests execute installed native/WoW64 leaves, including true
x86 `RET16`/`RET4` and 20 B/8 B total return-stack movement.

macOS owning CLI/MCP builds, seven CTests and both protected-text scans pass.
All three owning C++ selections pass. The applied Windows snapshot validates
17,250 files and 152 patches with no mismatch; the 374 B offline adapter is
unchanged. Final evidence prose differs from that compiled snapshot; the 56
recorded source units and two helpers match the compiled archive inputs.
Static-CRT Assist/core archives contain 1,122,525,464 B/735,226,360 B and
4673/260 structurally walked members. Core features remain empty.

The canonical 11,177 B observer is unchanged. It advances from turn 46,245 to
50,254: `50,254 - 46,245 = 4,009` scheduler calls. At thread 8,
`NtTraceControl` service `0x1D6`, PC `0x180002DA0`, captures
`[0x1B, 0xABE800, 4, 0, 0, 0xABE804]`. Smoke, MSVCRT streams, UCRT streams and
installed `whoami.exe` all retain internal failure before guest exit; the
reported exit field 0 is not an observed successful application exit.

## Assumption Register

| ID | Assumption | Basis/dependent result | Stress test | Falsification probe | Status |
|---|---|---|---|---|---|
| A1 | Build29683 ABI/validation applies to the selected caller profiles | PHNT, four-profile matrices; dispatch/copy | Aliases, LAA, crossing spans, guards | Any native/regression disagreement | Confirmed for measured profiles; earlier WoW copy assumptions revised |
| A2 | IDs require opaque generation/equality, not host-global numeric identity | Native IDs vary; local allocator | Recreate, exhaustion, update | A consumer needs ordering/arithmetic on host IDs | Retained |
| A3 | Fresh ARM64 Event effects apply to same-kernel x64/WoW callers | Shared kernel, mature calls; local Event model | All-ABI regressions and installed leaves | Completed fresh compatibility capture differs | Retained; fresh native compatibility unknown |
| A4 | Selected fresh process names have initial stamp 0 and empty data | 288 fresh cases; initial metadata | All nine names and masks | A successful fresh descriptor has nonzero stamp/data | Confirmed successful subscriptions; denied data unknown |
| A5 | Selection uses the creator primary subject for this bounded profile | Query-only token capture; immutable local admission | SID/group/deny-only/ACL order, token change | Primary-subject native access differs | Retained; impersonation outside profile |
| A6 | Captured grants 0/1/3 establish this mask conversion | 72 mature and 288 fresh calls; access admission | Publish-only or matched generic rights | Successful exact-grant-2 native oracle | Retained; grant 2 explicitly unmodeled |

## Change surfaces and bounded findings

| Plane | Result and evidence |
|---|---|
| Guest dispatch/objects/native acquisition | Changed: two exports, private catalog/subscription state, fixed registry/token snapshots and startup wiring |
| Runtime/targets/platforms | Changed source compiles on Windows/macOS/Linux; installed Windows acquisition requires Windows; built-in mode has no implicit host catalog |
| Owning plugin/CLI/MCP/C API consumers | Rust archives and five C++ selections validated per host; macOS CLI/MCP/CTest validated; full packages/live IDA/Qt unknown |
| Public SDK/C ABI/persistence | Unaffected: no public C fields, status/protocol/schema changes; C API1.11.0 and Assist Rust ABI184 unchanged |
| UI/lane/prompt/permissions/mesh/transport/crypto/release | Unaffected: no owning source or public contract edited; root changes limited to gitlink and two engineering records |
| ISA/MMU/SMIR/JIT/other personalities | Unaffected: no owning implementation edit; broad suites retain prior failures |
| Tests/docs | Changed: native-record replays, actual installed leaves, strict evidence checker, source/archive binding and engineering records |

| Impact | Finding | Blocks |
|---|---|---|
| High | Ordinary processes now require `NtTraceControl`; WNF data/delivery and later services remain | Full goal; not this measured increment |
| High | Impersonation, restricted/AppContainer subjects and unmodeled scopes/descriptors need further semantics | Broader WNF profile; fail-closed here |
| Medium | Fresh compatibility Event effects and physical Windows x64 proof remain unknown | Corresponding proof claims |
| Medium | Prior Linux timeout and Windows lowering failures remain | Full green-suite claim |
| Medium | Complete packages/live IDA/Qt execution is unproven | Complete product-platform claim |

## Provenance and quality gates

Prototypes are from the retained PHNT revision
`53fbbdc5b5d2b08761db1c7b26bfa8c820924356`; kernel behavior comes from the
retained installed native sources/27 producer identities, not PHNT prose.
The name layout also appears in Quarkslab's original research,
[Playing with the Windows Notification Facility (WNF)](https://blog.quarkslab.com/playing-with-the-windows-notification-facility-wnf.html).
Token acquisition uses Microsoft's
[GetTokenInformation contract](https://learn.microsoft.com/windows/win32/api/securitybaseapi/nf-securitybaseapi-gettokeninformation).
Guest execution never forwards these WNF requests to the host.

Run `python3 docs/specifications/windows/native-wnf-subscribe/check-evidence.py`
from RAX. It checks exact flat-file coverage, source/producer/PE identities,
native partitions/statuses/copy effects, fresh pending metadata, all 48 host
selections, original full failures, owning C++/archive/source bytes and the
unchanged ordinary observer. Controlled rejection logs test raw corruption,
rehashed forged native status, omitted manifest entry, empty selection, forged
summary and optimized Python.

QG1: no normative result; QG2: six assumptions with probes; QG3: measured
subscription/unsubscription and owning representations covered; QG4: fixed
widths, span ordering and scheduler-call arithmetic recorded; QG5: WoW64 alias,
guard and postpublication copy edges reconciled, broader unknowns explicit;
QG6: pinned primary/native/source/archive provenance; QG7: bounded findings and
affected/unaffected planes recorded. These gates apply to this increment; the
full same-host process-emulation goal remains active.
