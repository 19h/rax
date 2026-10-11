# Constructor provenance

The pinned [PHNT declaration](https://github.com/winsiderss/phnt/blob/53fbbdc5b5d2b08761db1c7b26bfa8c820924356/ntexapi.h)
supplies the ten-argument NtCreateWorkerFactory signature, information-class
names and basic-information layout. The exact header and applicable license
are retained as phnt-ntexapi.h and phnt-LICENSE; primary-retrieval.json records
retrieval time, commit, sizes and SHA-256 digests. PHNT is a native API declaration
source, not a Windows kernel implementation or a universal behavior guarantee.

The declared WORKER_FACTORY_ALL_ACCESS is0xF003F. Original installed Windows
10.0.29683.1000 observations grant specific bits0x40 and0x80 as well, producing
0xF00FF. That measured build owns this group's mapping; the declaration conflict
is explicit. Native statuses, capture/publication/guard order, process rights,
zero/nonzero maximum, raw entry/parameter/stack values, defaults and dormant
worker state are original native observations, not inferred from the signature.

The [Microsoft PE specification](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format#optional-header-windows-specific-fields-image-only)
places SizeOfStackReserve at optional-header byte72 for both PE32 and PE32+.
The old worker-factory-producer-identity.py incorrectly used byte88 on PE32+,
which names SizeOfHeapReserve. Its original artifact remains diagnostic evidence;
worker-factory-default-stack-identity.py and its six-row native output replace
that metadata with actual stack fields, producer/source hashes and machine/LAA
identities. No capture output or executable was regenerated for this correction.

No proprietary DLL, PDB, producer executable or archive is redistributed here.
Native query extent120 B (64-bit) or96 B (WoW64) is measured; the earlier128 B
query attempt correctly returned STATUS_INFO_LENGTH_MISMATCH and remains
preserved. Native pointer counts include fast-reference bias and change on
ordinary queries, so they are not used as exact kernel reference counts.
