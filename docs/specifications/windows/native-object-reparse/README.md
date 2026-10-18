# Captured object-manager directory links

The installed Windows namespace captures `Local`, `Global` and `Session` link
targets in selected base named-object directories. Guest parsing follows only
captured metadata into selected directories; it does not open a host handle.
Absolute and relative aliases retain the same directory identity. DOS KnownDllPath
values keep their distinct rejection, and uncaptured targets remain explicit.

The native matrix admits63 reparses and returns STATUS_OBJECT_NAME_NOT_FOUND
(0xc0000034) on the64th. OBJ_DONT_REPARSE returns STATUS_REPARSE_POINT_ENCOUNTERED
(0xc000050b) first. Directory OPENLINK flag refusal remains independent. Traversal
is bounded at63 expansions and32766 UTF-16 units. For H reparses, name length L
and directory membership E, time is O(H L log E) and temporary storage O(H L).

Native proof contains2432 directory-open rows and24 independent link-target
queries. All four ABI profiles use one Windows10.0.29683 ARM64 kernel; x64 and
x86 run through compatibility layers. Session-one paths were read with the same
session-zero principal. The corpus does not establish another token's grants or
other physical Windows kernel architectures. Producer sources, executable/source
identities, NTDLL hashes and complete handle closure are verified by check-native.py.
Seven rebound corruption controls are rejected.

| Host | Full library passed/failed/ignored | Focus | C API | Windows integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 7934/0/2 | 5 | 168 | 544; native-memory excluded |
| Linux x86-64 container | 7928/0/2 | 5 | 168 | 544; native-memory excluded |
| Windows ARM64 | 7373/5/2 | 6 | 168 | Four native-memory cases; Unix corpus excluded |

Windows retains exactly the five existing lowerer failures; the two ignored
microkernel tests remain explicit. All-target checks pass on all three hosts.
The native focus compares installed NTDLL directory-open leaves with independent
native opens on the host and WoW64 guest profiles. All5743 selected source/data
identities match before and after validation, rooted at published RAX
ce29b4a4bca74a668d7f029e8a69da6a2e0340e7.

Retained Assist checks pass seven CTests plus protected executable scans on macOS,
and five C++ consumers each on Linux and Windows. Two actual Cargo-selected
archives per host have complete member walks and empty core features;434
selected source/fixture inputs match. Frozen Rust carriers remain ABI184, with
compared C++ snapshot `e7fa3477850fbea5ce3ee819574f0a6946df172e` / ABI241.
The root publication snapshot `947212f548e05008902d972c19d5a5b7414840e2` uses
ABI261; eight RAX adapter/consumer source files match.
The independently changed static-text bridge stays outside retained-carrier
execution evidence. Whole-current-root GUI/plugin/package validation is not
established by these checks.

A program observer linked to the retained owner's actual core archive reaches:

- terminal turn2150070: NtCreateMutant (0xba) at0x7ff82fad1be0 is not implemented.
- terminal turn2152419: the same NtCreateMutant boundary.

Both unchanged ordinary programs still stop at unimplemented NtCreateMutant. Neither reaches a successful guest exit. Controller status0 is not a guest exit. The observer source, executable,
core archive and both unchanged targets have matching before/after identities.

This is a prerequisite for named synchronization objects. Native mutant services,
private BNO subtree capture and complete process execution remain unfinished parts
of the broader goal. Public C ABI, dependency pins, defaults and package layout
are unchanged. No real IDA database or user profile is modified.
