# Native TokenUser identity

NtQueryInformationToken class1 returns the immutable captured primary or modeled
impersonation subject's SID and opaque User.Attributes. Native acquisition now
retains that field rather than discarding it. Existing ACL interpretation and
other token selectors retain their contracts. Guest queries never forward a
host token handle or call the host kernel.

Native64 probes the entire declared output span at4-byte alignment, then the
mandatory ReturnLength DWORD, before validating token type/query access. It
writes required length before data. WoW64 uses private native scratch, performs
the SID copy before pointer/attribute stores, and writes optional ReturnLength
last. Its caller buffer is byte-aligned; direct faults preserve reached copy
progress and guard state. Native64 padding is untouched. The currently captured
12-byte SYSTEM SID requires28bytes(native64) or20bytes(WoW64). A revision1 SID
has8+4*N bytes,N<=15: required ranges are24..84 and16..76bytes, respectively.

WoW64 scratch failure precedes handle checks and sets Win32 LastError8. The
existing personality's no-paging-file temporary budget is reused without a
guest-sized host allocation or guest-visible scratch mapping. NULL output with
nonzero capacity bypasses scratch and follows its measured failure conversion.
Invalid handle/type/query-access errors publish the observed WoW64 translated
error length. Native reference observations, rather than SDK prose alone, own
these converter details. The documented native32-bit alignment/mandatory-length
contract and the observed WoW64 wrapper behavior are kept distinct.

The [TOKEN_USER definition](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-token_user)
and [NtQueryInformationToken contract](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntqueryinformationtoken)
are stored with retrieval metadata and SHA256. The attributes are retained as
opaque data; the captured native subject has0 and synthetic tests preserve
other values without assigning new ACL meaning.

## Verification

Four ABI profiles(ARM64,x64compatibility,x86,x86LAA) share one Windows10.0.29683
ARM64 kernel. The main matrix contains11handle kinds*(69lengths+43roles*9lengths)
*4profiles=20064calls. The boundary matrix contains2subjects*2protected-page
sides*4protections*41offsets*4profiles=2624calls. All22688records replay exactly,
normalizing only proven result-pointer bytes. Status, required length, raw SID,
attributes, padding, alias effects, LastError and fault-visible bytes remain
native observations. Twelve corrupted capture copies are rejected.

An additional36native controls retain last-NT-status while exercising ordinary,
NULL-buffer, invalid-handle and allocation-failure paths. Producer/source/PE
machine/LAA/module identities are bound before/after. Owned native handles
balance. Five shared tests compile and fail on the missing service before the
implementation. Final shared5 and Windows6(including real installed host/WoW64
leaves compared with an independently queried host SID/attributes) pass.

| Host | Unfiltered core passed/failed/ignored | C API | Process integration |
| --- | --- | --- | --- |
| macOS ARM64 | 8013/0/2 | 183 | 544 plus0cfg cases |
| Linux x86-64 container | 8007/0/2 | 183 | 544 plus0cfg cases |
| Windows ARM64 | 7467/5/2 | 183 | 0cfg cases plus4native |

All-target checks pass on all three hosts. The five Windows lowerer failures
match the exact established names. Actual retained Assist core/carrier archive
walks and C++ consumers pass on all three hosts, binding530selected source
inputs before and after links. macOS additionally passes seven exact CTests,
actual CLI/MCP carrier linkage and protected-text checks. Retained Rust ABI184
and the prior eight-C++-input comparison to root ABI264 do not establish
whole-current-root GUI/plugin/package coverage. Public RAX C API1.14.0,
dependencies, defaults and public layouts are unchanged.

The complete pre-gate source epoch binds23442inputs/1695720818bytes. Its
post-gate runtime engineering append is separately bound. All Rust changes
preceded final complete gates. Native CallerLastStatus controls and receipts
are evidence additions; they do not change compiled inputs.

## Ordinary execution and bounds

Both ordinary guests return TokenUser success at former turns2883974/2886259,
publish required length28bytes, advance2743normally executed observer turns,
and stop at the uncaptured per-user Safer/CodeIdentifiers key under S-1-5-18.
New terminal turns are2886717/2889002. Neither guest exits successfully. Native
controls exit0; native whoami output is unchanged. The observer links the exact
validated retained-owner core: 503968678 bytes, SHA256
`b3ca4f9e090a00f7421860c941c45f50f7a77b7a8373625abbc6d273662ca784`. Source, executable, programs, six runtime modules and archive
identities remain stable before/after. Controller success is separate from
guest success.

The native User matrix has one SYSTEM SID; other SID sizes/opaque attribute
values have synthetic layout coverage. Their short-copy order reuses existing
native copy evidence and remains an explicit inference. Physical host-heap
exhaustion equivalence is untested; temporary backing follows the existing
guest budget. These limits, missing-capture refusal, unchanged ACL policy,
single-kernel scope and the next registry frontier are recorded in
validation/final-assumptions.json. Full same-host process emulation remains
incomplete.

Run python3 -B check_native.py . here and python3 -B check_validation.py /path/to/rax.
The evidence manifest excludes itself. Three-host verification receipts state
their original checked-envelope scope; later receipts and verbatim whitespace
audit are explicitly appended afterward.
