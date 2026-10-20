# Allocation-wide native memory queries

NtQueryVirtualMemory classes 3 and 7 now report the guest reservation extent,
allocation protection, backing type, private commitment and NUMA preference.
The implementation uses guest memory state. It does not forward guest addresses
or handles to the host kernel. C ABI 1.14.0 and the public RegionInfo path,
layout and existing basic-query behavior are preserved.

The observed output contract comes from the sources and raw captures under
`native/`: fifteen successful producer versions, four ABI profiles per version,
one Windows 10.0.29683 ARM64 kernel. The x64 executable profile runs on that
kernel; it is not an independent physical x64 kernel observation.

| Profile | Pointer bytes | Class 3 minimum bytes | Class 7 minimum bytes | Full bytes |
|---|---:|---:|---:|---:|
| ARM64, x64 | 8 | 24 | 32 | 48 |
| x86, x86 LAA | 4 | 16 | 20 | 28 |

Successful copies publish complete pointer-sized prefixes and preserve the
remaining bytes. Version 16 checks every length from 0 through 65 bytes for
both classes. Earlier versions cover rights, misalignment, invalid handles,
free and upper addresses, optional ReturnLength, aliasing and guard effects.
Native 64-bit and WoW64 capture/error ordering differ; the shared implementation
retains those differences at the ABI boundary.

| Allocation origin | Class 3 type | Class 7 flags |
|---|---:|---:|
| Private | 0x00020000 | 0x1 |
| File section | 0x00040000 | 0x2 |
| Image | 0x01000000 | 0x4 |
| Pagefile section | 0x08000000 | 0x8 |

Private commitment includes committed private pages and copy-capable mapped
pages. A permitted write access acquires a persistent per-page charge even
when the stored bytes are identical or a later syscall check fails. Unwritten
copy potential disappears under read-only protection; acquired charge remains.
Initial shared writable PE sections differ from an explicit later writable
protection. Initial pagefile WRITECOPY views also charge reserved section pages.
Versions 3 through 12 and 15 isolate these transitions. Opt-in interval metadata
and two unused page-table bits carry this state; Linux and Darwin mappings do
not opt into Windows copy accounting. No content-difference heuristic is used.

Versions 13 and 14 establish reservation-owned NUMA preference. Explicit node 0
is retained through decommit/recommit. A commit into an existing reservation
does not replace its preference. Release/reuse clears it. Ordinary allocations
report zero-extended 0xFFFFFFFF. The modeled topology has one node.

The regression first compiled and failed both unsupported-class checks, then
passed after implementation. The final compiled replay contains 1,608 native
observations: 1,256 matrix, 304 boundary and 48 rights cases. Its masks exclude
only allocation-dependent base/size/commit bytes; aliased ReturnLength bytes,
fixed fields, status, copy length, guards and untouched bytes remain checked.
Separate lifecycle and instruction-level tests cover the masked quantities,
cold/warm/raw writes, concurrent acquisition, remap, discard, unmap and reuse.

| Complete library binary | Passed | Failed | Ignored |
|---|---:|---:|---:|
| macOS | 8099 | 0 | 2 |
| Linux | 8093 | 0 | 2 |
| Windows | 7560 | 5 | 2 |

The five Windows failures are the exact previously recorded AArch64 lowerer
cases, listed and checked in the receipts. Each host also runs 183 C API tests,
the selected integration binaries (544 cases on macOS/Linux, 4 on Windows),
all-target compilation and retained Assist ABI 184 consumers. Seven macOS
CTests and six recorded link commands, and five C++ consumer checks on each
other host, exercise the rebuilt core/carrier boundary. Sources and structural
archive identities are checked before and after those consumers.

Actual owning-core execution and the independent Assist C++/CAPI path both run
the ARM64 smoke executable to exit status 0. Installed ARM64 whoami advances
past twelve successful class 3 queries and stops at unimplemented
NtQueryAttributesFile service 0x3d. CAPI reason 5 denotes failure; its separate
exit-code field is not a successful guest exit. Native control whoami exits 0
with `nt authority\system` followed by CRLF. The four executed artifacts have
independent retained copies with matching hashes.

`validation/` binds 25,082 frozen source paths (1,835,096,144 bytes), the 3,823
owning source inputs, commands, producers, raw outputs, assumptions, rejected
setups and final review. Producer version 9 failed because a scheduler slot was
already occupied; version 10 clears the initial slot before the valid capture.
The failed compile iteration, source-archive directory retry and cancelled
observer-label setup remain explicitly separate from successful validation.
Raw evidence is protected against Git text conversion.

Run `python3 -B check_validation.py --host macos --source /path/to/rax` (or the
corresponding Linux/Windows host value). The checker reconstructs every replay
row from the raw logs, verifies exact full-suite selections and consumer
identities, and rejects new failures or claims of whole-goal completion.
Source verification costs O(B) time for B frozen bytes with bounded streaming
buffers; replay reconstruction costs O(N) time/space for N captured rows.

The process-emulation goal remains incomplete. Current-root ABI 270 products,
GUI/plugin/package execution and the separate CI session-directory symbolic
link condition are outside the retained ABI 184 consumer claim. The actual CI
alias target remains unknown. These boundaries and impact-labeled findings are
recorded in the final review; they are not platform-support exceptions.
