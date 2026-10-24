# Current-process UI context query

`NtUserGetProcessUIContextInformation` returns two DWORDs in an 8-byte output
buffer for the selected current-process personality. Field semantic names are
unknown. The model captures the fixed native current-process query on its own
GUI helper and verifies stability across input acquisition. Guest arguments
never enter a host query. Missing query metadata does not disable unrelated GUI
operations; an attempted query reports its unavailable state explicitly.

Cold calls enter the actual guest USER32 callback 84 with zero input bytes.
Only a successful callback status resumes the interrupted query. Handle
validation follows conversion and precedes buffer access. Failed conversion
returns zero without querying or copying output. Callback output parameters
are not dereferenced by the measured continuation.

| Subject | Query result | LastError | Output access |
| --- | --- | --- | --- |
| Current-process pseudo handle or current virtual process with grant `0x1000` | 1 when output is writable | Preserved | Exactly 8 bytes |
| Invalid handle, wrong object type or insufficient grant | 0 | 87 | None |
| Valid current subject and inaccessible output | 0 | 998 | No successful copy |
| Valid current subject and guarded output | 0 | `0x80000001` | Guard consumed, output unchanged |
| Another valid virtual process without captured context | Explicit unavailable diagnostic | Preserved | None |

Real native `PROCESS_QUERY_INFORMATION` opens imply the limited-query grant in
the measured OS. The virtual object lookup consumes its recorded grant; it does
not send virtual process identifiers or handles to the host. Native zero-access
`OpenProcess` attempts failed with error 5 and therefore provide no query result
for a real zero-grant handle.

The fixed cohort contains 4 executable views * 2 threads * 7 available handle
roles * 3 repetitions * 15 buffer cases = 2520 queries. Stack offsets 0 through
7 succeed for permitted subjects and change exactly 8 bytes. A crossing
read-only destination preserves the complete poisoned output, including its
writable prefix. Invalid subjects leave guarded output guarded. On native64
ARM64/x64 paths, buffer faults set LastNtStatus to `0xC0000005` or
`0x80000001`; success and handle failures preserve it. The x86 TEB32 status
remains preserved; paired native64 WoW64 TEB state was not measured.

Five accepted cold-v1 controls cover actual callback execution, controlled
success/failure/warning, and invalid callback-output arguments. Four additional
v1 mode names were rejected by that producer with exit 10 and empty logs.
They are excluded from API observations. Corrected cold-v2 runs the four modes
successfully: a null output yields error 998 after conversion; invalid process
handles yield error 87, including when output is also null.

The selected ARM64 `win32kfull.sys` function is at RVA `0x8ECC0`:
section RVA `0x1000` + public-symbol offset `0x8DCC0`. Its body references
process Win32 state at offset `0x320`, extracts two fields bounded 0 through
3, and copies 8 bytes. The observed current-process fields are `[0, 0]`.
The package binds the matching disk image and PDB. This is static disk-image
evidence, not a read of the loaded kernel or its process-state field.

Twelve process-DPI setter attempts returned access denied. Twelve further fresh
manifest variants across four executable views all reported effective process
and thread DPI awareness 2 despite differing embedded manifests. These controls
establish neither field semantics nor independence from DPI. They remain private
with source, embedded manifests, receipts and log identities.

All native views execute on one Windows ARM64 10.0.29683 kernel in SYSTEM
context. Independent x64-kernel and interactive-user behavior are unknown.
Raw handles, identifiers, stack/TEB/GDI/desktop buffers and loaded code remain
private. Published observations contain only scalar/relative results, hashes,
producers and receipts.

For a converted thread, lookup costs O(log H + log M + log T) time for H virtual
handles, M memory mappings and T GUI threads, with O(1) temporary space. The
8-byte copy touches at most two 4096-byte pages. Cold callback execution has its
own guest execution cost and shared GUI mapping lifecycle.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| UI-A1 | Captured current-process context is the initial input for this selected guest personality | Fixed native query and before/after helper capture | Initial guest query output | Changed process/image policy; changing capture | Ordinary bootstrap rejects input or native policy produces unrepresented fields | Retained; setters and general policy relationships unknown |
| UI-A2 | Conversion precedes handle validation, then output access | Corrected four-mode cold cohort and hot guard controls | Deferred completion and validation order | Invalid handle plus null/guard output; callback failure | Guard consumed by invalid subject, or validation occurs before callback | Confirmed for measured controls |
| UI-A3 | Native64 copy faults project recorded TEB status/error | Four-view fixed cohort | Error/status and guard mechanics | Repeated guard, crossing read-only page, unaligned output | Same-profile native transition contradicts the projection | Confirmed for selected kernel; WoW64 pairing remains unknown |

| Impact | Remaining boundary | Effect |
| --- | --- | --- |
| High | Whole USER/GDI behavior and ordinary process completion | This query does not establish complete process userland support |
| High | Guest context policy mutation and other virtual subjects | Require their own state and native observations |
| High | Paired WoW64 GUI contexts | Remain explicitly unavailable |
| Medium | Independent x64 kernel, interactive-user context and in-memory kernel state | Bound native portability and provenance claims |
