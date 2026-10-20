# Native file attribute queries

`NtQueryAttributesFile` and `NtQueryFullAttributesFile` operate on authorized
mapped files and selected installed runtime DLL basenames. General filesystem
permission remains separate from native-library selection. Guest pointers,
handles, security descriptors and unchecked flags are not forwarded to the host.

Basic output has four 64-bit FILETIME fields and 32-bit attributes:
`4 * 8 + 4 = 36` bytes. Native output adds four padding bytes, totaling 40 bytes;
WoW64 copies 36 and preserves its tail. Full output is `6 * 8 + 4 + 4 = 56` bytes.
Native destinations require eight-byte alignment; WoW64 accepts arbitrary byte
alignment. Time units are 100 ns since 1601-01-01; sizes are bytes. Windows reads
actual NTFS allocation size, including the observed 8-byte allocation for a
5-byte resident fixture, rather than rounding to a guessed page or sector.
Unix timestamps use `116444736000000000 + seconds * 10000000 + nanoseconds / 100`
FILETIME ticks with integer sub-tick truncation; the conversion loses less than
100 ns of precision. This is a representation bound, not a clock-accuracy claim.
Unix allocated bytes are exactly the reported `st_blocks * 512` calculation.

Guest name/security capture and validation take `O(U + S)` time and space,
where `U` is the counted name size and `S` is the captured security data.
Case-insensitive fallback enumerates host directory entries per path component;
its work and OS metadata latency depend on the selected filesystem. Fixed output
serialization uses at most 56 bytes. No constant filesystem-latency bound is
claimed.

Native producer v10 contains 426 roles for each of two APIs over four ABI views:
`426 * 2 * 4 = 3408` observations. Its 231 controlled fault pairs contribute
`231 * 2 * 4 = 1848`. Native ARM64/x64 pair results agree; x86/x86-LAA pair results
agree. All four views use one Windows 29683 ARM64 kernel. They do not establish
independent physical x64-kernel equivalence. The compiled pair corpus executes
`231 * 2 * 3 = 1386` dispatcher requests per host, with expected statuses and
output-page protection taken from independent native captures.

The native and WoW64 capture stages differ. The corpus distinguishes header
reads, character reads, security-child validation, QoS semantics, guards and
dual faults. Compiled red checks retain absent services (0 passed / 3 failed),
the prototype's wrong WoW64 output alignment (0 passed / 1 failed), and its wrong
capture order (0 passed / 1 failed). Earlier broad prototype results are historical
measurements, not final-source proof. Rejected compiler/controller iterations
are recorded separately.

NT spelling is preserved without Win32 dot/space normalization. Runtime canonical
containment retains leaf spelling for OBJ_DONT_REPARSE. Mapped/selected roots are
admitted namespace boundaries. Retained host files support empty-name queries
after pathname replacement. Legacy metadata-only HLE objects retain their existing
pathname-identity limit. Device/UNC/stream names and unavailable/unrepresentable
host metadata remain explicit frontiers. External host namespace mutation does
not acquire an atomicity guarantee.

Actual owning-core and retained Assist C++/C API runs determine program outcomes.
Final-source smoke exits with code 0; whoami advances to NtOpenFile (0x33).
A FAILED result's separate zero exit-code field is not a successful guest exit.
The full process objective remains active. The original source ledger covers 25,097 files / 1,835,033,720 bytes. Windows
uses its separately bound 374-byte Cargo configuration in place of the original
552-byte file: `1835033720 - 552 + 374 = 1835033542` native source bytes.
Final-source counts/identities belong to quality.json and its closed envelope. Retained Assist ABI184 consumers do not
establish complete current-root GUI/plugin/package behavior.

The first native Windows corruption-runner replay accepted a changed hash in
the original Cargo ledger entry because the native override branch checked only
its separate 374-byte configuration. The verifier now also pins the original
552-byte entry before applying that override. The rejected trial and subsequent
platform revalidation are retained in the audit-correction record. Product source
and the compiled/query/owning results are unchanged by that verification fix.

The prior alias stress runner cached scratch identities, so later rejections could
precede semantic checks. Its 13 controls were revalidated with all scratch paths
excluded from the cache and a passing restored corpus before each corruption.
The current stress runner enforces that boundary and positive-restoration check.

Public projections omit complete private Assist compiler messages before first
publication. Original receipts retain checkpoint hashes. Only exact declared
published-byte projections resolve to those identities. Complete new originals
remain private. No history rewriting is claimed.

Run `python3 -B check_validation.py --source-root <frozen-source-root>` to audit
source bytes, native observations, complete selections, archive walks, actual
owning results and projections. Native Windows also hashes the four retained
owning artifacts; other hosts report that byte check as unperformed. See
assumptions.json for the Assumption Register and bounded findings, and validation/
for native observations and rejected iterations.

Primary contracts: [NtQueryAttributesFile](https://learn.microsoft.com/en-us/windows/win32/devnotes/ntqueryattributesfile),
[full attributes](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-zwqueryfullattributesfile),
[basic layout](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/ns-wdm-_file_basic_information),
[network-open layout](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/ns-wdm-_file_network_open_information),
accessed 2026-10-20. Probe/conversion/error ordering is measured native evidence,
not an inference from those layout documents.
