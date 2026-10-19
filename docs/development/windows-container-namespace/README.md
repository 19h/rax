# Windows container session namespace selection

An independent native `NtQueryDirectoryObject` / `NtQuerySymbolicLinkObject`
probe on Windows Server 2022 (10.0.20348) observed:

| Object | Type | Target |
|---|---|---|
| `\BaseNamedObjects` | Directory | - |
| `\Sessions\34316` | Directory | - |
| `\Sessions\34316\BaseNamedObjects` | SymbolicLink | `\BaseNamedObjects` |

34316 is the observed process session ID, not a fixed selection value.
The previous capture opened the last object as a directory, following its link,
and then rejected it because its parent had captured a symbolic link. This
prevented native runtime selection in a container before guest execution.

Capture now retains the session link's type, target, and access grants. When
its target is exactly the already captured global `\BaseNamedObjects`
directory, no duplicate directory is opened or attached. Guest lookup uses the
existing bounded symbolic-link parser. Other targets, absent link grants, and
missing global selection remain errors. Ordinary session directories retain
their previous capture path; no host filesystem or service grants are added.

The selector costs O(D + E) time per candidate and O(1) additional retained
space for D selected directories and E entries in the selected session parent,
apart from bounded path formatting. Both collections retain the existing
capture bounds.

Portable unit tests cover ordinary directories, session IDs 1, 34316 and
`u32::MAX`, shared directory identity, link query/type, permission ceilings,
no-reparse behavior, and targets outside the captured global directory.
The existing native process-tool test exercises actual runtime selection.

Primary API references: the retained [NtQueryDirectoryObject source](ntquerydirectoryobject.md.gz) (gzip, byte-preserved)
and [provenance](sources.json), plus the existing
[NtOpenDirectoryObject](../../specifications/windows/native-object-directory-open/references/NtOpenDirectoryObject.html),
[NtOpenSymbolicLinkObject](../../specifications/windows/native-symbolic-link-open/references/NtOpenSymbolicLinkObject.html),
and [NtQuerySymbolicLinkObject](../../specifications/windows/native-symbolic-link-open/references/NtQuerySymbolicLinkObject.html)
references.
