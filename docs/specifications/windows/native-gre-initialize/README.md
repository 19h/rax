# Installed GRE initialization registry selection

The installed Windows runtime selects the fixed root
`\Registry\Machine\Software\Microsoft\Windows NT\CurrentVersion\GRE_Initialize`
through the existing complete, bounded, immutable subtree adapter. The parent
actual owning caller rejected this independently present root as outside the
runtime snapshot. Selection now captures its raw values and descendants, or
records independently established root absence. Other acquisition errors fail.
Guest registry operations use owned records and never forward guest names or
pointers to host registry APIs. The existing read-only guest grant policy remains.

The production change adds one `TreeRoot` variant, its fixed path and one
installed adapter selection. Public C API 1.14.0, its header, dependency pins,
feature defaults, permissions, persisted formats and package layout are unchanged.
The shared implementation and all affected targets compile on Windows, macOS and
Linux. Native acquisition runs on Windows; all three hosts execute shared guest
semantics and the captured data through the existing kernel-service dispatcher.

```text
fixed native root -> complete tree / root absence / acquisition error
                           |
                  immutable selected records
                           |
              guest NtOpenKey and NtQueryValueKey
                           |
                  owned key/child handles
```

Original native observations cover 144 opens per execution view:
`4 spellings * 4 attribute masks * 9 access masks = 144`.
ARM64, x64, x86 and x86 LAA give `4 * 144 = 576` opens. Each execution also
captures all three registry views twice, giving `4 * 3 * 2 = 24` complete trees.
The final repeat uses the same hashed executables and doubles both totals.
All observations use one Windows 11 ARM64 build 29683 kernel; an independent
x64-kernel result is unknown.

Every tree contains three keys and seven values: `180 B` of UTF-16 value names
plus `142 B` of raw data, totaling `322 B`. Complete native paths occupy `502 B`.
Default, 64-bit and 32-bit views, both samples and all execution views agree,
including the recorded native metadata. DWORD zero and six raw string payloads
retain their native types and complete bytes. Names and data are not normalized.
Native timestamps bracket acquisition consistency; this change does not add
new guest key-metadata fields.

All 576 original opens and all repeats preserve the recorded status, exception
and last-error fields. Successful host handle numbers are opaque and are not
required to equal guest handle numbers. Native handle counts balance. The
recorded host permits write-capable open grants; the established guest registry
policy denies them. Tests replay the 112 admitted read/zero-grant rows per view
and separately require denial of the two write-capable masks. This is an explicit
existing policy boundary, not an assertion that host ACLs are copied.

The seven shared focused tests cover confinement, absence, raw ownership,
duplicate selection, aggregate limits, complete captures, service read grants,
relative child lifetime, raw value serialization and the existing write denial.
Three additional native Windows tests compare actual installed capture with
independent native reads, distinguish root-not-found from other errors and
exercise controlled present capture plus disappearing descendants without
creating or modifying a host key.

Existing limits remain 4096 combined values and 16 MiB of combined value-name
and data bytes; each selected tree permits 1024 keys, depth 32, 255 UTF-16 units
per component and 1 MiB of path bytes. For V0 previously selected values and
K keys, V values, P path bytes and B name/data bytes in this subtree, capture and
construction cost O(V0 + K + V + P + B), plus existing per-node sorting. Added
retained space is O(K + V + P + B). The existing case table is shared by handles.

The Win32 API definitions are primary references in `sources.json`:
[RegOpenKeyExW](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regopenkeyexw),
[RegEnumValueW](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regenumvaluew)
and [RegEnumKeyExW](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regenumkeyexw).
The original native producer and immutable observations establish this installed
profile; those references do not specify the undocumented GRE subtree contents.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| GRE-A1 | This fixed root shares contents across the selected registry views | 24 original and 24 repeated complete captures | Shared root selection | Three registry views, four execution views, all descendants and raw values | Different view contents or incomplete capture | Confirmed for this profile; other kernels remain separate |
| GRE-A2 | Only root ERROR_FILE_NOT_FOUND establishes absence | Existing adapter contract and native error tests | Fail-closed selection | Other errors, changing enumeration, disappearing child, prefix/sibling names and aggregate limits | Any error converted into successful partial/absent capture | Confirmed for exercised paths |
| GRE-A3 | The next mapped instruction corresponds to NtGdiInit2 | Same eight code bytes and RVA as the registered Win32u export | Next-boundary description only | Independently mapped image outside module catalog | Backing-image/section comparison disproves correspondence | Confirmed for code/RVA correspondence; full mapped-image identity unknown |

| Gate | macOS ARM64 | Linux x64 container | Windows ARM64 |
| --- | --- | --- | --- |
| GRE focused tests | 7 passed | 7 passed | 10 passed |
| Unfiltered library | 8,203 passed, 0 failed, 2 ignored | 8,198 passed, 0 failed, 2 ignored | 7,678 passed, 5 failed, 2 ignored |
| C API | 183 passed | 183 passed | 183 passed |
| Windows integration targets | 544 passed | 544 passed | 4 passed |
| All targets | Passed | Passed | Passed |
| Retained Assist consumers | Eight linked targets, seven CTests passed | Five C++ consumers passed | Five C++ consumers passed |

Windows's five failures exactly match the established ARM64 lowerer failures.
The full Windows suite is not green. Existing ignored tests remain explicit;
none was added or skipped for this change. Linux x64 uses translation on an
ARM64 host and is not a physical x64 instruction oracle. Rustfmt 1.95, strict
library Clippy and ordinary all-target Clippy with `x86_64-suite` pass.

The initial macOS full run reports five failed fork-dependent tests during a
host `Resource temporarily unavailable` outage. All five exact tests then pass
against the unchanged test image; the complete unfiltered rerun also passes.
Linux's initial compiled ordinal pass is followed by a failed process launch;
the first retry finds OrbStack stopped. The existing service is started and the
complete Linux gates pass on the same frozen implementation. Original failed
receipts, raw logs and the independent recheck remain retained. No assertion,
profile expectation, timeout or test selection was weakened.

The final compiler frame contains 29,180 inputs and eleven overlays against
`a9ca828fa18ab92a8a3ed866da97f360030b079c`: seven Rust files and four captured
logs. Frame SHA-256 is
`e2994771bbad47110bbe6c7ed3b9a2973711e2388f3cf7fa612169cdfa7ce122`.
The 3,981-input owning envelope matches that frame. Actual Cargo-selected
core/carrier archives have complete structural walks and stable identities
before and after linked consumers. The retained Assist ABI184 consumer proof
and current-root ABI274 GUI/plugin/package CI are separate surfaces.

The owning-source ledger inherited stale procfs descriptive labels, while its
selected source rows and nested frame binding identified the preceding read
checkpoint. Current records replace those labels and bind the current parent
and frame. Original historical records remain unchanged. Source hashes, selected
compiler artifacts and executable behavior own attribution over descriptive labels.

The compiled parent regression reaches the independently present GRE root at
Rust observer index 2872 and fails the selection assertion with exit 101. The
same observer source passes after the change. Smoke remains `Complete(Exited(0))`.
Whoami reaches index 2875; the fresh C++/C API caller reports 2876 started turns.
Both therefore advance three normally executed observer turns beyond the former
boundary. Native controls exit 0 and all recorded program/runtime/core/carrier/
source/executable identities remain stable before and after execution.

The new terminal instruction is in an image mapping at allocation base
`0x3D10000`, outside the registered module ranges. Its RVA is `0xBFD0`, giving
`0x3D10000 + 0xBFD0 = 0x3D1BFD0`. Eight instruction bytes at that site match the
registered `win32u.dll` leaf whose exact export is `NtGdiInit2`. The selected NTDLL
service table does not admit service `0x12FD`, so execution fails explicitly.
This correspondence is diagnostic; no new service admission or host call is
introduced. C API reason 5 denotes failure despite its zero exit-code field.
Whoami has not completed successfully under emulation.

| Plane | Effect and boundary |
| --- | --- |
| Registry model and Windows adapter | One fixed root through existing bounded immutable capture; actual Windows acquisition plus shared replay |
| Guest dispatch, grants and object lifetime | Existing dispatcher/serialization/access policy; new root data, handle and namespace-teardown tests |
| Linux/macOS process adapters | Unchanged source and contract; shared guest behavior compiles and executes on all three hosts |
| ISA, CPU state, SMIR, optimizer, lowerers, JIT, devices and analysis output | No implementation changes |
| Assist lifecycle, UI, conversation, agent/context, tool schema, permission policy, dispatch, Mesh/MCP, transport, persistence and SDK | No changed contract or source |
| ABI, update, discovery, build options and packages | Existing opaque process API and linked code; no runtime asset, layout, dependency, default or package change |
| Targets and evidence | Library, C API, integration/all-target gates and retained actual owning consumers on three hosts; current shipping CI separate |

| Impact | Remaining boundary | Effect |
| --- | --- | --- |
| High | Win32u NtGdiInit2 correspondence and broader same-host process userland | Whole goal remains active; no successful whoami emulation claim |
| High | Foreign process ownership and further unsupported kernel behavior | Separate ownership/model work remains |
| Medium | One ARM64 kernel profile and independent x64 behavior | Profile limits remain explicit |
| Medium | Established lowerer and x64 profile/time failures | Retain exact failures; no weakened assertions |
| Medium | Transient host process failure and stopped Docker runtime | Original failures retained separately from complete reruns |

Native probe outputs are removed after final hashes and process-use checks.
The generated negative-control copy is removed. Compatible in-worktree caches
have concrete next use for Win32u service admission, owning process validation
and remaining lowerer/profile work. No normative judgment is required. The full
Linux/macOS/Windows same-host process userland objective remains incomplete.

The evidence/source verifier and final receipt/control checker pass on macOS,
Linux and native Windows against one frozen proof/checker pair. Eleven data
corruptions and both optimized-Python modes reject; both positive cases pass.
The output receipts live in `validation/`, outside the indexed verifier inputs,
to avoid self-reference. Run `check_evidence.py` and then
`validation/check_validation.py` to verify input evidence and these outputs.
