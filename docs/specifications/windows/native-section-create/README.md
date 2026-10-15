# Native unnamed pagefile-section creation

This bounded step implements NtCreateSection construction, not section mapping,
query services, or full Windows process completion. The original native debug
child retains all seven syscall arguments and status, creates65,536bytes with
PAGE_READWRITE/SEC_COMMIT and no name/file, and naturally exits41. The previous
owning observer stopped at this service after2,115,129 scheduler calls.

The two final native controllers produce10,920 creation observations and close
1,516 returned section handles across ARM64,x64,x86,x86-LAA on ARM64 kernel
10.0.29683.1000. Compatibility execution is not a physical x64-kernel oracle.
The priority matrix contains1,386 cases/profile; the supported-protection and
allocation cross contains1,344 cases/profile. Inspectors verify actual compiled
source,PE machine/LAA,executable and raw transferred stdout. Earlier exploratory
controllers remain outside this final corpus.

The portable model replays10,876 exact native status/output/maximum/grant and
available metadata results. The remaining44 cases assert specific unsupported
admission for unimplemented modes; they do not silently skip. Five further
constructor tests cover page rounding,zero fill,last-handle lifetime,aliased
maximum/output capture,duplicate identity,status translation and refusals.
The Windows-only test executes installed native and x86 NTDLL leaves,including
RET28 cleanup. Full-target,C API,platform and owning-embedding results are added
only after their commands complete.

The outer allocation mask and nonzero PAGE syntax are checked before probing
outputs or reading MaximumSize. PAGE=0 reaches a later validation stage.
WoW64 converts OBJECT_ATTRIBUTES first and copies a zero native handle after
kernel errors,except when the output pointer is null. Valid section sizes are
positive,rounded to4,096bytes,and bounded to67,108,864bytes. Rounding is
`ceil(size/4096)*4096`; the bound proves the addition cannot overflow u64.
A constructor uses O(1) Rust metadata plus an owned sparse host backing of the
rounded length; frames are supplied by the existing host backing adapter.
No guest name,pointer or handle reaches a host NT section-creation service.

NOCACHE suppresses WRITECOMBINE in reported attributes. WC alone rejects
executable protection after size capture. Generic section rights differ from
event rights; unknown specific rights are masked by0x001F001F,while MAXIMUM_ALLOWED
maps to0x000F001F. A SYSTEM-oracle success does not authorize guest security
privileges: ACCESS_SYSTEM_SECURITY remains an explicit frontier.

Supported creation stores SEC_COMMIT/SEC_RESERVE and cache attributes and owns
real zero-filled anonymous backing. Mapping remains explicitly unsupported in
the existing mapper. Named/file-backed sections,custom security/QoS,permanent,
large-page/based/64K/no-change semantics and larger backings are separate work.
NtQuerySection has not been added; the native oracle's query validates stored
metadata. No successful guest exit is inferred from a terminal internal error.

Primary documented contract: Microsoft ZwCreateSection(wdm.h),last updated
2023-01-13; Microsoft requires the Nt name for user-mode callers. Raw HTML and
retrieval/hash provenance are retained. Private build29683 capture and failure
ordering come from the independent native controls,not the model. Status
constants also agree with the existing mingw-w64 reference headers.

Run `python3 check.py` for artifact/producer/count integrity and
`cargo test --locked --offline --no-default-features --lib section_create_tests -- --nocapture`
for the focused model and native-host-selected leaf tests.
