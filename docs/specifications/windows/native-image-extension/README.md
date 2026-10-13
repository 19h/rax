# Native image-extension query over guest-owned PE images

NtQueryVirtualMemory class 14 now implements the measured absent-extension
contract for complete guest PE images. Full same-host process execution remains
an active incomplete goal. The unchanged ordinary observer advances
97,257 -> 114,140: 114,140 - 97,257 = 16,883 scheduler calls.
These are scheduler calls, not instructions or a successful process exit.

| Ordinary program | Current service | Class | Instruction PC | Stopped-thread PC | Result |
|---|---|---:|---|---|---|
| ARM64 smoke.exe | NtSetInformationProcess, 0x1C | 112 (0x70) | 0x1800011D0 | 0x1800011D4 | reason 5, internal failure |
| ARM64 MSVCRT streams.exe | NtSetInformationProcess, 0x1C | 112 (0x70) | 0x1800011D0 | 0x1800011D4 | reason 5, internal failure |
| ARM64 UCRT streams.exe | NtSetInformationProcess, 0x1C | 112 (0x70) | 0x1800011D0 | 0x1800011D4 | reason 5, internal failure |
| Installed ARM64 whoami.exe | NtTraceEvent, 0x5E | n/a | 0x180001620 | 0x180001624 | reason 5, internal failure |

All four report exit_code 0 as part of the internal failure record; no guest
exit is proved. The first three now reach class 112 with an 8 B input.

```text
97257: NtQueryVirtualMemory(class 14)
   |
   +-- measured absent extension: RVA 0, size 0, STATUS_SUCCESS
   |
114140: NtSetInformationProcess(class 112) -> explicit unsupported frontier
```

The private PHNT declaration names class 14 MemoryImageExtensionInformation.
Microsoft's public NtQueryVirtualMemory page documents class 0 only. Declaration
provenance does not establish private fault precedence. Native build 29683
captures establish the ordering and bytes here; other builds remain unknown.
The retained primary snapshots and PHNT MIT license are bound in
references/sources.json and evidence-manifest.json.

| ABI field | Native64 offset | Extent |
|---|---:|---:|
| ExtensionType (MEMORY_IMAGE_EXTENSION_TYPE) | 0 B | 4 B |
| Flags (ULONG) | 4 B | 4 B |
| ExtensionImageBaseRva (PVOID) | 8 B | 8 B |
| ExtensionSize (SIZE_T) | 16 B | 8 B |
| Total | 0 B | 24 B |

Layout arithmetic: 4 B + 4 B + 8 B + 8 B = 24 B. Type 0 is CFG/SCP;
type 1 is emulated CFG/SCP; values >= 2 are invalid. For an actual native image
without an extension, both valid types succeed with zero RVA and zero size.
The caller's type/flags remain unchanged; a non-NULL ReturnLength receives
24 as an 8 B value after the 16 B output store. Aliases retain that store order.

Current guest image owners already advertise ImageExtensionPresent = 0 in
class 6. Class 14 describes that actual state. No host kernel CFG/SCP mapping,
code-integrity decision, signing level or host executable bytes are inherited.
An extension owner, if required later, must implement mapping, relocation,
lifetime and both class 6/class 14 queries together. Native installed NTDLL has
extension state: ARM64 type 0 yields RVA 0x473000 and size 0x1000 B; x64
compatibility type 1 yields RVA 0x474000 and size 0x1000 B. Those present-state
controls are explicitly excluded from absent-image equivalence.

The installed WoW64 conversion rejects class 14 with
STATUS_INVALID_INFO_CLASS (0xC0000003). Its existing optional 4 B ReturnLength
preprobe runs first: a reached guard is consumed; an inaccessible destination
is ignored. Class 14 then does not capture the output/type, does not publish
ReturnLength, and does not probe the output. A hypothetical 16 B 32-bit struct
from declaration arithmetic does not imply an implemented WoW64 conversion.

Native64 observed algorithm:

```text
NULL BaseAddress -> INVALID_PARAMETER
length < 24 B -> INFO_LENGTH_MISMATCH
BaseAddress outside guest user range -> INVALID_PARAMETER
output not aligned to 8 B -> DATATYPE_MISALIGNMENT
output extent outside user range / arithmetic overflow -> ACCESS_VIOLATION
probe the entire declared output extent for writing
probe optional 8 B ReturnLength, with byte alignment
validate handle, process type and query rights
reserved Flags != 0 -> INVALID_PARAMETER
no allocation at BaseAddress -> MEMORY_NOT_ALLOCATED
allocation is not Image -> INVALID_ADDRESS
ExtensionType >= 2 -> INVALID_PARAMETER
write zero RVA/size, then optional ReturnLength = 24 -> SUCCESS
```

Both PROCESS_QUERY_INFORMATION (0x400) and PROCESS_QUERY_LIMITED_INFORMATION
(0x1000) suffice individually. PROCESS_VM_READ alone does not. Pseudo-current
process succeeds; pseudo-current thread is a type mismatch. Other process
owners remain an explicit unsupported boundary after rights validation.
The current-process query adds no guest/native handles or new virtual allocation owner.
The released-image boundary no longer describes the previous image.

Range rejection occurs before any page visitation, so overflow/out-of-user-range
lengths cannot consume guards. Legal large extents probe pages until the first
fault, without allocation proportional to guest SIZE_T. Let P be visited pages,
A allocation-map entries, H handle-map entries, and T the existing memory
subsystem translation/population cost per visited page. Query time is
O(P*T + log(A+1) + log(H+1)), with expected constant object-ID hash lookup.
Query scratch is O(1); fixed successful output is 24 B. Shared AddressSpace
may materialize backing for already committed pages, up to P*4096 B. Numeric
range validation is O(1) time/space.
The existing guard wrapper records one reached guard transition and preserves
output/ReturnLength/protection effects before later status resolution.

| Native family | Cases/profile | Profiles | Captured rows | Exact portable replay rows |
|---|---:|---:|---:|---:|
| Allocation/type/flags/length matrix | 4,200 | 4 | 16,800 | 14,280 |
| Probe/alias/guard/handle/rights order | 15,480 | 4 | 61,920 | 61,920 |
| SIZE_T limits, overflow and guard priority | 1,092 | 4 | 4,368 | 4,368 |
| Total | 20,772 | 4 | 83,088 | 80,568 |

83,088 - 2,520 = 80,568 exact replays. The 2,520 controls comprise two
native64 caller profiles * three NTDLL target positions * six types * five
flag values * fourteen lengths. The independent checker reconstructs all axes
and ordinals rather than trusting a producer-supplied case inventory.
Same-width ARM64/x64 and x86/x86-LAA results agree in status, output, optional
length and page state, with native SCP payload differences explicitly scoped.
Each successful native producer finishes its entire partition with exit 0;
query handle counts are restored. Four profiles are ARM64, x64 compatibility,
x86 WoW64 and x86 WoW64 with LARGE_ADDRESS_AWARE on an ARM64 kernel.
Physical x64-kernel execution remains unknown.

There are 13 actual compiled producers: three families * four profiles, plus
one fresh ARM64 debugger producer. native-image-extension-final-producer-identities.json
binds the actual compiled source, PE machine/LAA, executable and stdout.
The older nine-producer identity file is retained as a historical capture;
it does not replace the final thirteen-producer inventory.

The fresh debugger installs at the original installed MEM_IMAGE service entry,
observes actual SVC 0x23 and the true LR return, and records startup classes
6, 4 and 14. It retains one excluded warmup and two complete fresh cases with
exact handle-set restoration. Class 14 before/after bytes, addresses, thread,
function and returned length are checked. Native child exit 77 denotes this
controlled producer, not an emulated ordinary process exit.

An aborted first boundary producer failed before any case after OpenProcess
with desired access 0 returned ERROR_ACCESS_DENIED. The subsequent version-2
producer obtains the zero-access current-process handle through DuplicateHandle
and captures all cases. The aborted source/build/outcome and an initial wrapper
filename error are retained as diagnostic cache evidence, excluded from complete
native producer counts. No valid captured producer source was edited afterward.

Initial emulator baselines fail class 14 explicitly. The first implementation
exposed NULL-address/short-length precedence and SIZE_T range rejection before
ReturnLength guards; native capture corrected both. A replay harness correction
restores the WoW64 caller SP between raw dispatcher calls, which stop before
NTDLL RET cleanup. Native rows and assertions remain intact. Diagnostic baseline
and failing-pass logs are retained separately from final gates.

| Host | Focused passed | Full passed/failed/ignored | C API | Integration | C++ consumers |
|---|---:|---|---:|---:|---:|
| macOS | 5 | 7735/0/2 | 168 | 544 | 5 |
| Linux | 5 | 7729/0/2, unfiltered serial | 168 | 544 | 5 |
| Windows ARM64 VM | 6 | 7145/5/2 | 168 | 4 | 5 |

All-target builds pass on every host.

Windows's five failed tests are the exact four BZHI and one FP16 lowering
baseline failures from the preceding published group. No Windows emulator test
fails. Two pre-existing ignored microkernel checks remain counted separately.
The Linux unfiltered full selection is serial; earlier groups' parallel timer/
EINTR diagnostics remain in their original immutable corpora. The cause of those
historical parallel failures remains unknown. No assertion or shared policy is
weakened here. Installed leaf tests execute actual ARM64 and WoW64 NTDLL leaves
against guest-owned images, including x86 return stack cleanup.

The native shipping archive is 1133524966 B, SHA-256
`a958df26772394d2369f8b4c654edd813857220d217571b38784522c744c5405`, with 4,673 members.
The Cargo-selected empty-feature core is 745855982 B, SHA-256
`477326fbba054c662c8c36224d3527db164b265863572c3b0278573b51d53888`, with 260 members.
Complete member walks, 92 actual Rust source identities and two actual helper
identities pass. The observer is 11,177 B, SHA-256
`d41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e`.

Owning Unix embeddings use frozen root e2fc5b483be7cad2dae17d889690cc398973c5e8;
Windows uses f386ed2aeac29a701d32f28eb73c14003eb55f7a. The nine current process
adapter/header/factory/consumer files match those embeddings byte-for-byte.
Concurrent root Cluster/Kubernetes/lifetime changes and other root deltas are
excluded in owning-root-scope.json. This evidence validates current RAX semantics
and selected consumers; it does not establish the full current-root plugin,
GUI or package matrix. macOS CLI/MCP, seven selected CTests and both protected
static-text scans pass. Both other hosts' five C++ consumers pass.

Primary, native-producer, actual archive and source hashes are verified.
check-evidence.py passes independent partitions, byte extents, native identities,
same-width comparisons and fresh original-entry/true-LR/lifetime proof. All
17 check-negative-evidence.py controls reject the intended falsification for
its exact reason. Hash re-binding cannot hide changed input axes, row deletion,
status/protection/output changes or fresh entry/thread/payload changes.

assumption-register.json, bounded-scope.json and change-surface-map.json identify
dependent conclusions, falsification probes, all owning planes and remaining
full-goal boundaries. QG1-QG7 pass for this semantic group: no normative content;
reconciled A1-A3; exact captured scope; explicit ABI/range arithmetic; independent
partitions and edge cases; verified primary/runtime provenance; bounded findings.
These gates do not declare the full process-emulation goal complete.
