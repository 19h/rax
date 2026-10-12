# Declaration and native evidence provenance

PHNT commit53fbbdc5b5d2b08761db1c7b26bfa8c820924356 supplies the declaration
in the retained ntexapi.h, with its license, retrieval timestamp and hashes:
[PHNT ntexapi.h](https://github.com/winsiderss/phnt/blob/53fbbdc5b5d2b08761db1c7b26bfa8c820924356/ntexapi.h).
NtCreateTimer2 takes PHANDLE, optional PULONG TimerId, optional
PCOBJECT_ATTRIBUTES, ULONG Attributes, ACCESS_MASK DesiredAccess.
Its declaration constants identify IR2, HIGH_RESOLUTION4, NO_WAKE8 and
NOTIFICATION0x80000000. The header does not establish parameter priority,
UserMode ID authority, access mapping, pointer alignment or fault ordering.
Those claims come from the four original native producer families and raw logs,
not from a reconstructed status table or the emulator implementation.

The selected host is ARM64 Windows11 build29683.1000. ARM64 is native;
x64 and x86 are compatibility processes on that ARM64 kernel. x86-laa is an
independently linked LARGEADDRESSAWARE profile. Physical x64 Windows proof is
unknown. Disabling all privileges affects only each reduced producer process;
it does not establish token/SID-independent authorization. Different callers
or kernel builds can falsify the retained IR-ID role profile.

Producer field count is Timer2 Attributes, not a timer count or thread maximum.
Captured addresses and HANDLE values are process-specific. Replay relocates
only the explicitly initialized nested OA pointer in modes24..26 and overlapping
HANDLE publication bytes, validating their shape/native value first. Every
other retained byte, status, grant, guard and lifecycle count is compared.

The reduced producer records WaitForSingleObject(handle,0) return0x102 and
IRTimer object names. Its inline GetLastError expression has unspecified C++
argument evaluation order; wait-error is retained as diagnostic data and does
not establish the error for that wait. The wait return and object query are
separate observations. NtQueryTimer on successful new handles returns
STATUS_OBJECT_TYPE_MISMATCH0xC0000024, so legacy Timer semantics cannot supply
this constructor's representation. No arming, expiration or APC claim follows.
