# Native system time

This group implements `NtQuerySystemTime` for the same-host native Windows
userland. Ordinary application execution remains incomplete; its final result
must be recorded after the continued non-test observer runs.

The selected native ARM64, x64 compatibility, x86 and x86-LAA probes cover 205
calls each, 820 total. They agree on ULONG (4-byte) alignment before memory
access, an atomic 8-byte output, preserved last-error state, and the observed
write-protection/guard ordering. Writable guard pages are consumed; a readonly
guard refuses the write without consuming the guard. A cross-page failure
leaves both halves of the output unchanged. These are observations on Windows
10.0.29683.1000; the x64 compatibility producer is not a physical x64 kernel.

The native service uses the existing real-time clock adapter and the existing
FILETIME conversion used by `GetSystemTimeAsFileTime`. For a Unix sample of
`s` seconds and `n` nanoseconds, the result is
`(s + 11,644,473,600) * 10,000,000 + floor(n / 100)` ticks, each 100 ns, since
1601-01-01T00:00:00Z. This preserves the current host-clock policy; it introduces
no seed-derived clock or guest clock setter. Four-byte alignment is a measured
output-pointer constraint, separate from the eight-byte result width.

[Microsoft's declaration](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntquerysystemtime)
defines the signature, time origin and units. It does not specify all pointer
fault ordering. The native producers, raw transcripts and post-run identities
in `native/` provide those observations. `check-native.py` checks the complete
case axes, raw input hashes, producer identities, clock bounds and caller bytes.
The runtime regression replays every recorded case through all four ABI
profiles and normalizes only successful clock values; it compares status,
protection, last-error and the remaining caller bytes exactly.

The assumption register and bounded-scope records distinguish the service
contract, the short validation clock brackets and unfinished ordinary startup.
