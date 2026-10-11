# Primary contract excerpts and provenance

Microsoft, CreateIoCompletionPort, retrieved 2026-10-11 UTC:
https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-createiocompletionport

> If this parameter is zero, the system allows as many concurrently running threads as there are processors in the system.

This Win32 contract supports projecting a zero requested concurrency onto the
modeled guest processor count. It does not prove runnable-thread scheduling in
the emulator. That behavior remains a distinct requirement. The native producer
independently measures acceptance of all nine tested unsigned 32-bit counts;
NtQueryIoCompletion's basic information exposes depth rather than concurrency.

PHNT, winsiderss/phnt, ntioapi.h, pinned commit
53fbbdc5b5d2b08761db1c7b26bfa8c820924356, retrieved 2026-10-11 UTC:
https://github.com/winsiderss/phnt/blob/53fbbdc5b5d2b08761db1c7b26bfa8c820924356/ntioapi.h

The retained phnt-ntioapi.h and phnt-LICENSE define the four-argument native
constructor signature. Native status, private probing order, flag mask, grants,
byte publication and guard behavior come from the original producer outputs on
Windows 10.0.29683.1000 rather than claims inferred from the public signature.
Proprietary NTDLL files, PDBs and producer executables are not redistributed.
