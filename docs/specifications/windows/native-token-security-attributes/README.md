# Captured token security attributes

This engineering record covers `NtQuerySecurityAttributesToken` on the installed
Windows 10.0.29683 ARM64 kernel and its ARM64, x64 compatibility, x86 and x86 LAA
ABI views. Four ABI profiles are observations of one kernel, not four kernels.
It does not establish successful ordinary-program exit or a complete Windows
userland personality.

The private `Subject` snapshot now owns optional version1 attributes. `None`
means capture is unavailable; it never means an empty token. Installed-runtime
selection queries the same private query handle as the existing subject capture,
validates every pointer against the returned allocation, and copies names and
all seven declared value types. The existing double-capture consistency check
includes the resulting owned values. Guest queries never call a host token API.

## Measured contract

`primary/phnt-ntseapi.h` declares the API and structures. Native programs and
their original source, compiler output, PE identities, module identities and
outcomes are under `native/`. `check_layouts.py` is independent of product code.
It verifies 22656 completed observations, including bytes, status, ReturnLength,
guard transitions and LastError. The unsuccessful v4 and v7 compiles are retained and excluded from behavioral
counts; v4b is the corrected seven-type fixture and v7b captures empty sets.
The 28 rejected zero-value creation controls supply no successful query rows.
The v3/v5/v6/v9 matrices compare their captured 128-byte prefix and complete
recorded status/protection/ReturnLength state; they do not capture every byte of
every supplied capacity.

| Representation | Native64 | WoW64 |
|---|---:|---:|
| Information header | 16 bytes | 12 bytes |
| Version1 attribute | 40 bytes | 24 bytes |
| UNICODE_STRING | 16 bytes | 8 bytes |
| FQBN element | 24 bytes | 16 bytes |
| SID/octet element | 16 bytes | 8 bytes |

Native validates output pointer/length consistency, four-byte output alignment
and the entire output extent before ReturnLength. WoW captures non-NULL caller
descriptors first, permits an unaligned descriptor array and uses private native
output. Both then validate four-byte ReturnLength alignment and accessibility,
capture the names, and check handle type and TOKEN_QUERY rights. Empty names
are invalid; unknown names return `STATUS_NOT_FOUND` with ReturnLength zero.
Length drives the input capture; MaximumLength does not bound this query.
Case folding uses the selected Windows upcase table without path normalization.
Duplicate requested names remain duplicated; missing names prevent a partial
successful result.

The measured positive query-all contains `TSA://ProcUnique`, UINT64[2], flags
0x41. Its two values are opaque process identity and are captured unchanged.
Native required capacity is 104 bytes; WoW requires that native capacity before
returning its converted length, 88 bytes at an eight-byte aligned destination.
At a destination congruent to four modulo eight, converted length is 84 bytes.
This follows absolute alignment, not a separate policy:

```text
native: 16 + 40 + 32 = 88; align8(88) + 16 = 104 bytes
WoW:    12 + 24 + 32 = 68; align8(68) + 16 = 88 bytes
WoW+4:  12 + 24 + 32 = 68; align8(4+68)-4 + 16 = 84 bytes
```

Nonempty packing clears the supplied capacity. An empty native result writes
only its 16-byte fixed header with a NULL attributes pointer and preserves the
remaining capacity. WoW requires native capacity16, clears the caller capacity,
and publishes a 12-byte header with a NULL attributes pointer. UTF-16 text aligns to two bytes; value
arrays align to eight absolute address bytes. Native logical required length
does not include a caller's alignment bias: 104-byte native capacity at base+4
returns `STATUS_BUFFER_OVERFLOW` and a partially populated attribute, with its
information header and values pointer still zero. The information header is
published after payload. ReturnLength is stored last, so a return pointer that
aliases the header overwrites the corresponding header field. A WoW copy fault
instead retains native required capacity, published before caller copying.

The additional 4608 boundary observations establish the measured zero-store
plan consistent with the captured prefixes and fault states: first unaligned
16-byte store, absolute 16-byte aligned body, then the final unaligned 32-byte
pair. This is a compatible store plan, not an instruction trace of NTDLL. Each store can fault independently. This preserves
visible partial zeroing and one-shot guard consumption.

## Scope and assumptions

| ID | Assumption and dependent result | Basis | Stress test and falsification probe | Status |
|---|---|---|---|---|
| A1 | This installed kernel defines the measured compatibility scope, including native/WoW fault and packing order. | 22656 completed native observations, four ABI views. | Independent byte/status/protection checker and dispatcher replay; any differing row falsifies the corresponding rule. | Retained within build29683. |
| A2 | Selection can retain full attributes as owned values, including opaque identity. | Positive TSA attribute and private-token fixtures for all seven types. | Decode captured pointer graphs; reject escaped/truncated/unknown types; compare installed leaf output with independent native query. | Confirmed within the measured scope: all seven types, empty sets, 13 native focused tests and independently decoded installed leaves passed. |
| A3 | Normalized attributes can participate in the existing bounded subject consistency loop. | Pointer-free values with Eq; existing acquisition checks two snapshots, at most three attempts. | Installed selection and repeated native leaf calls must remain stable. Pointer relocation changing equality or lost token/WNF capability falsifies it. | Confirmed for installed selection and both ordinary programs; no guest query reaches a host API. |

Selection capture, encoded results and aggregate captured name bytes are each
bounded to 1 MiB, with at most 4096 attributes or requested names. Individual
empty-valued attributes have no successful query fixture: all 28 private ADD
attempts with ValueCount0 failed. Acquisition rejects such unmeasured records;
an empty attribute set is supported separately. Exceeding a capture bound produces an explicit unavailable frontier; it
does not synthesize native failure status or empty data. Buffer capacity never
controls a host allocation. A packed query uses O(B+A+V) time and O(B) scratch
for B bounded encoded bytes, A attributes and V values. Guest output zeroing is
O(L) time for supplied capacity L, with constant-size zero scratch.

## Change surfaces

| Plane | Effect |
|---|---|
| Guest native service and DLL exports | New six-argument service and installed NTDLL leaf; x86 callee cleanup is 24+4=28 bytes. |
| Token ownership | Private optional Subject field; primary and impersonation Arc subjects retain identical owned attribute semantics. |
| Installed Windows adapter | Query-only selection-time capture and existing stable-snapshot comparison. |
| Guest memory | Measured alignment, descriptor/string capture, zeroing, guard visitation, alias and publication order. |
| ISA decode/execute, SMIR, optimizer, lowerers, JIT | Unchanged; this adds an OS service boundary through existing CPU stop and ABI transport. |
| Public Rust/C ABI, persistence, dependencies, feature defaults | Unchanged; no public field, serialized schema, dependency or feature change. |
| Assist plugin/CLI/MCP/harness | Consume the same embedded RAX core; retained consumer/link gates remain required. No Qt/IDA scheduling or tool-schema change. |
| Windows/macOS/Linux | Shared service and models compile and replay on each; installed capture and real NTDLL leaves run on Windows. Native host-library availability remains the existing platform contract. |

## Validation state

The initial tests-only baseline compiled and ran three failing tests before the service
was added: each stopped at the unimplemented native service, not a compile error.
The independent native checker passes all 22656 observations. Two further compiled regressions established the empty-set pointer/tail error
and the upper-address WoW copy-order error before their corrections. The final twelve shared focused tests pass on unchanged before/after source
inputs, including 22656 native query rows and explicit resource-bound tests.
The uncorrupted evidence copy passes and fifteen logical/identity corruptions
are rejected. Final source epoch2 has 24786 files / 1808385701 bytes. The twelve shared focused tests and thirteen native Windows focused tests passed, including installed native/WoW leaves and x86 28-byte return cleanup.

| Host | Full passed | Failed | Ignored | C API | Integration |
|---|---:|---:|---:|---:|---:|
| macOS | 8049 | 0 | 2 | 183 | 544 |
| Linux | 8043 | 0 | 2 | 183 | 544 |
| Windows | 7512 | 5 | 2 | 183 | 4 |

The five Windows failures exactly match the established lowerer failures in the saved native gate; no additional failures are accepted. Integration counterpart binaries with zero tests are explicitly distinct from the populated selections. All-target compilation passed on all three hosts. Retained Assist Rust ABI184 consumers passed seven macOS CTests, five Linux consumers and five Windows consumers. Source identity covers 880 selected inputs and the complete frozen source graph; actual Cargo core/carrier archive structure, member counts, hashes before/after consumers and six macOS linker commands are saved. This does not establish whole current-root ABI264 plugin, GUI, package or release validation.

The actual owning core has 506136090 bytes, SHA-256 `41fa4b161609e88a2bf24943be1755e0dea8bfb65cab1048f60e85e64e9c736f`. Both ordinary ARM64 guests return `STATUS_NOT_FOUND` / ReturnLength0 for the missing SYSAPPID/PKG names, then continue normal execution: smoke2899173->2943177 (+44004 observer turns) reaches unsupported `NtTestAlert (0x1d3)`; whoami2901458->2966197 (+64739) reaches unsupported `NtQueryVirtualMemory` class3. Neither guest exits successfully. Both native controls exit0; source, observer executable, program, core and six installed DLL identities match before/after. Candidate observations and the distinct owning observer are retained separately.

The first native source preparation stopped on 109 missing zero-byte parent files; none had a wrong nonzero hash. The corrected overlay supplies exactly the verified empty files and preserves the parent source. macOS epoch1 socket failures were reproduced with an overlong temporary path and all 16 socket tests passed under a short in-tree path; final full gates used that path. A retained Windows build-helper omission and a metadata filename mismatch, plus macOS Python-path archive churn, are preserved failed harness attempts; final consumers use stable input and archive identities.

`check_validation.py SOURCE` checks native observations, primary reference identity, source inputs, compiled regressions, all three full selections, exact existing failures, actual retained consumers and ordinary-program evidence. Logical corruption controls rebind modified log hashes before checking. Portable proof receipts and publication are separate steps.


## Bounded findings

| Impact | Finding | Blocking scope |
|---|---|---|
| High | Both ordinary ARM64 guests pass the token query and reach NtTestAlert / NtQueryVirtualMemory class3; successful guest exit remains unproved. | Blocks the full emulation goal. |
| Medium | One kernel's fault and copy order does not establish other Windows revisions. | Limits compatibility evidence; no unmeasured universal claim. |
| Medium | Captures or requests beyond the explicit bounded schema remain unavailable. | Limits supported service inputs; no fabricated success. |
| Low | Adjacent WorkerFactory classes0/1 remain unsupported on the measured installed kernel. | Outside this token service change. |
