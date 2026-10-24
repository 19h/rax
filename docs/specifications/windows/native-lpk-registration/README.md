# Process-local LPK registration

`NtUserRegisterLPK` records a complete DWORD in guest process state. It returns
1 after successful GUI-thread conversion. A cold call retains its argument in
the pending kernel-to-user continuation and commits it only after a successful
callback return. Failed conversion returns zero and leaves earlier completed
registrations intact. A nested registration during an outer callback completes
independently; the outer operation keeps its own argument and continuation.

The selected ARM64 `win32kfull.sys` disk-image function is at RVA `0x95A80`.
Its matching public symbol is in section 1 at offset `0x94A80`:
`0x1000 + 0x94A80 = 0x95A80`. Import-table resolution identifies calls to
`EnterCrit`, `PsGetCurrentProcessWin32Process` and
`UserSessionSwitchLeaveCrit`. The body stores W0's saved 32-bit value at process
state offset `0x2F0`, then returns 1. It does not dereference the input value.
This is static disk-image evidence. The corresponding kernel-memory field was
not read by the native controls, and its initial value remains unknown.

The guest represents the value as absent until a modeled registration
completes. It does not infer an initial zero or copy a host registration into
the new guest process. The setter does not expose a getter or implement other
language-pack services. No guest argument reaches a host kernel operation.

The native fixed cohort spans four executable views, two threads, three
repetitions and sixteen DWORD inputs:
4 * 2 * 3 * 16 = 384 calls. Every call returns 1 and preserves LastError
`0x12345678` and LastNtStatus. Inputs include zero, individual low/high bits,
`0x7FFFFFFF`, `0x80000000`, `0xFFFFFFFE`, `0xFFFFFFFF` and `0x13572468`.
The x86 leaves end with `RET 4`, agreeing with one DWORD stack argument.
ARM64, x64, x86 and large-address-aware x86 execute on one Windows ARM64
10.0.29683 kernel; independent native x64-kernel behavior is unknown.

| Cold ARM64 control | Callback status | Return | Conversion marker after | Native observer exit |
| --- | --- | ---: | --- | ---: |
| Actual USER32 callback | `0x00000000` | 1 | Present | 0 |
| Controlled success | `0x00000000` | 1 | Present | 0 |
| Controlled failure | `0xC0000001` | 0 | Absent | 5 |
| Controlled success, output pointer 1 and `UINT32_MAX` bytes | `0x00000000` | 1 | Present | 0 |
| Controlled warning with the same invalid output | `0x80000001` | 0 | Absent | 5 |

Every cold control begins without the native conversion marker, invokes
callback 84 with zero input bytes and preserves LastError `0x12345678`.
The measured continuation does not dereference callback output. Native observer
exit 5 in deliberately failed USER32 loads is distinct from C API reason 5,
`RAX_PROCESS_FAILED`. Only reason 4, `RAX_PROCESS_EXITED`, has a valid guest
exit-code field.

The registration update costs O(1) time and O(1) process state. Shared GUI
conversion has its separately measured mappings and thread lifecycle. Tests
cover full DWORD preservation, successful/failed cold continuation, nested
registration commit order, unavailable inputs, invalid x86 argument capture and
actual ARM64/x64 guest instruction paths through the callback dispatcher.

Raw stack, TEB, desktop, GDI and code-leaf captures remain private. Published
projections retain scalars and original log identities. Native producers use
fresh processes and preserve source, executable and installed-runtime hashes.
The selected kernel disk image was rechecked after the native cohorts; this
does not constitute an in-memory kernel measurement.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| LPK-A1 | Registration retains a full process-local DWORD | Selected function, resolved process-state import and STR W19 | Guest state type and scope | High-bit inputs, repeated and worker calls | Different selected body or observed width/scope | Confirmed statically for selected image; native field contents not read |
| LPK-A2 | Cold registration resumes after thread conversion | Five native cold controls | Deferred commit and return value | Failure, warning, invalid output and nested guest calls | Return or marker mismatch; lost outer operation | Confirmed for recorded ARM64 controls; nested commit order follows recorded process store and shared conversion ordering |

| Impact | Remaining boundary | Effect |
| --- | --- | --- |
| High | Whole USER/GDI behavior and ordinary process completion | This registration operation alone does not complete the process objective |
| High | Paired WoW64 GUI conversion and unimplemented language-pack operations | Remain explicit unsupported boundaries |
| Medium | Independent x64 kernel and direct kernel-state observation | Limits native portability and field-state claims |
