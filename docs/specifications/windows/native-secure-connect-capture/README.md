# Native secure-port capture and namespace failures

The original nine `NtSecureConnectPort` arguments are captured without host
IPC. The selected immutable namespace determines missing-name/path results.
A captured ALPC-port entry establishes presence; it does not create a guest
connection. Present and unselected endpoints remain explicit unsupported
boundaries, with no output handle published. Successful CSR connection and
shared-view ownership remain separate requirements of the full goal.

Four initial producers provide 88 native observations each on Windows ARM64
10.0.29683.1000. x64, x86 and x86 large-address-aware are compatibility ABIs
on this kernel. The independent inspector checks actual source/executable/PE
identities, transferred stdout, every status, caller SEH, 64-byte failure
buffers, view/QoS/SID/string input bytes, per-call handle counts and cleanup.
There are 396 observations, zero caller exceptions and twelve explicit closes.
A separate fixed read-only NTDLL query measures C0000078 -> Win32 error 1337.

The corpus covers pointer, read-only and cross-page faults; name bounds/case;
QoS lengths and modes; 48-byte and WoW64 24-byte client-length capture;
24-byte remote input; ignored section identity/geometry before missing-name
lookup; SID validation; message/data/length capture; and competing faults.
It records WoW64's zero HANDLE copyback after reached native failures and
unchanged output after early wrapper faults. The actual 24-byte client view
success ABI was separately measured, but success is not admitted by this group.

Caller data is read in 4096-byte chunks with O(B) probe time and O(1) scratch
space for B information bytes. The transient bound is 1048576 bytes; larger
requests remain unsupported and preserve outputs. The raw name byte-vector
payload is at most 65534 bytes; the decoded vector adds at most 32767 UTF-16
units (65534 bytes). Namespace lookup adds O(N) scratch for N name units.
These are payload bounds, excluding allocator metadata. Namespace traversal
uses the captured catalog and never
connects a guest request to a host service.

Run `python3 check.py` to verify native artifact hashes and observations.
Rust replay tests execute all 396 cases and compare exact outputs, unchanged
handles/objects/committed bytes, and continuation state. Native Windows tests
also execute installed NTDLL leaves and verify the ninth argument and RET36.
Platform build/runtime results belong in post-run validation records; this
pre-run contract does not assert that any new gate has already passed.

The additional 44 read-only observations cover every caller parameter on all
four profiles. Client and remote views and nonempty information spans require
write access even for a missing endpoint; the information-length counter only
requires read access on this failure path. WoW64 view/data write faults still
produce its zero-HANDLE copyback. Input names, QoS and SID permit read-only
backing. These controls exposed and corrected omissions in the first draft.
First-pass validation is retained under epoch1; corrected source is epoch2.

Completed three-host validation, retained failures and exact source/archive
identities are recorded in [validation/README.md](validation/README.md).
