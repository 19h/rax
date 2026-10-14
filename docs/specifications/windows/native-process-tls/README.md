# Native process TLS replacement

This corpus records the installed Windows10.0.29683 ARM64 kernel under SYSTEM,
S-1-5-18. It implements NtSetInformationProcess class35 in the Windows personality
using guest memory and guest threads. No guest pointer or operation reaches host NT.
Full ordinary Windows process completion remains unproven.

The installed compact layout differs from the pinned PHNT declaration. Native
ARM64 and x64 use the same24-byte entry; WoW64 uses a12-byte caller entry and
captures a native-width packet before the kernel phase. Evidence comes from1040
native calls across ARM64, x64, x86 and x86 with LAA, plus one captured ARM64
loader entry and return. x64/x86 debugger attempts are retained as unsuccessful
captures; the direct controls completed on all four profiles.

| Field | Native64 offset/bytes | WoW64 caller offset/bytes |
|---|---|---|
| Flags |0/4 |0/4 |
| OperationType |4/4 |4/4 |
| ThreadDataCount |8/4 |8/4 |
| TlsIndex or previous vector count |12/4 |12/4 |
| First entry Flags |16/4 |16/4 |
| First entry pointer union |24/8 |20/4 |
| First entry ThreadId |32/8 |24/4 |
| Subsequent entry stride |24 |12 |

For N entries, native64 requires16+24*N bytes with N>=1. It checks caller
alignment8, current-process pseudo-handle and structural length before visiting
the buffer; a structurally possible length then probes the entire writable
caller span before checking the header count. WoW64 tolerates unaligned callers
and ignores the supplied length. Its converted temporary backing uses the
personality's existing no-paging-file budget.

Only the current-process pseudo-handle is accepted. Flags0 selects the native
packet; native64 rejects Flags1 after a matching length. WoW64 retains the
caller's low flag bit; higher bits fail with STATUS_INFO_LENGTH_MISMATCH. Every
entry flag must initially be0, including unused entries.

Operation0 replaces the selected TLS slot, returns its old pointer in the entry
union and marks the consumed entry2. Operation1 copies the requested old-vector
prefix into the supplied new vector, publishes it in the TEB, returns the old
vector and thread identifier, and marks the entry2. A null old vector skips a
thread. The startup sentinel at the TEB pointer field has a null old-vector
result and requires no prefix copy. The packet capacity limits enumeration;
remaining eligible threads retain their vectors.

Native64 writes directly to the caller packet. Prefix copying can overwrite an
earlier caller-field store when a new vector aliases that packet. WoW64 captures
its original packet and copies converted results back only after kernel success.
A later-thread fault retains earlier TLS changes. Native64 retains earlier
caller-field changes too; WoW64 leaves its original packet intact on that kernel
failure. A subsequent WoW64 caller-copy fault retains completed TLS changes.

The adapters use O(N+T*log(T)+K*P) time, O(T+P) temporary bytes on native64 and
an additional O(N) captured bytes on WoW64: T process threads, K selected threads,
P copied prefix bytes. Thread identifiers increase with creation; the extracted
scheduler owner participates in the same enumeration as its peers.

Run the independent portable verifier:

```sh
python3 -B docs/specifications/windows/native-process-tls/verify-evidence.py
```

The1040 controls comprise176 original calls,408 extended calls,136 alias/partial
thread calls and320 structural-length ordering calls. An accidental duplicate
collector run contributes176 excluded calls. Failed initial debugger and alias
fixtures are retained. The alias fixture captures then restores the affected
TEB fields before teardown; its `teb-after` snapshot represents the kernel phase.
For sentinel aliases, the later `vector-after` read occurs after restoration.

The strategic tests exercise all three guest transports, startup sentinel and
null vectors, slot and prefix replacement, partial thread failure, caller alias,
structural lengths, readonly copyback and guard state. Replay compares all176
original native statuses and guard results,160 readable caller packets and172
readable vectors. Unreadable control snapshots are excluded from byte assertions.

Primary PHNT declarations and license are pinned in [primary-contracts.json](primary-contracts.json).
The nominal PHNT layout is a historical declaration; it is not the installed
kernel contract established by this corpus. DLLs and native executables are not
redistributed; their identities are bound by the independent native inspectors.

The broader goal still requires NtApphelpCacheControl, NtTraceEvent, complete
mixed ARM64EC/x64 execution and successful ordinary Windows guest exits.
