# Installed NLS sorting registry snapshot

This increment selects the fixed runtime registry subtree
`\Registry\Machine\System\CurrentControlSet\Control\Nls\Sorting\Versions`
during Windows native-library enrollment. The actual owning Rust and C API
whoami routes requested this key with `KEY_READ` after the volume-device query
increment. The change uses the existing bounded immutable subtree capture,
ordinal case table, key objects and native/WoW64 registry services. It adds no
host registry callback during guest execution.

Only `TreeRoot::NlsSortingVersions` is added. The `Nls` and `Sorting` parents,
adjacent keys, similarly prefixed names and uncaptured roots remain outside the
selected namespace. A missing selected root or descendant is distinguishable
from an unselected name. Guest handles own their captured values and children
after the selection namespace and ancestor handles are dropped.

The existing adapter opens query/enumeration handles, compares two complete
value/child enumerations and before/after metadata, and closes a parent before
opening children. Root `ERROR_FILE_NOT_FOUND` selects absence. Other errors,
disappearing enumerated children and unstable snapshots fail acquisition.
Guest strings never select native paths. No registry mutation is introduced.

Existing bounds remain unchanged: 4,096 aggregate values and 16 MiB
(16,777,216 B) of aggregate value/name bytes; 1 MiB per value; 16,383 UTF-16
units per value name. Each selected subtree has at most 1,024 keys, depth 32
and 1 MiB of retained path bytes. The native 65,536-entry ordinal upcase table
is shared. For K captured keys, V values and B retained bytes, acquisition and
retention use O(K + V + B) work/space, apart from bounded sorting and the shared
case table. Each key's repeated enumeration has at most three attempts.

## Native observations and policy boundaries

Independent C++ controls use the installed NT and Win32 APIs, without product
code. The measured ARM64 Windows kernel is build 10.0.29683. Four execution
views (ARM64, AMD64, x86 and x86-LAA) each record three registry views, two
complete raw value enumerations per view, and 80 NT key opens. These are views
of one kernel, not independent physical AMD64-kernel evidence.

| Observation | Captured result |
|---|---|
| Fixed key | present; zero children; 10 raw values |
| Default, WOW64 64-bit, WOW64 32-bit registry views | identical raw names/types/data |
| Absolute name, trailing separator, lowercase name | successful native opens |
| Missing child, repeated separator/missing descendant, suffix, dot components | `STATUS_OBJECT_NAME_NOT_FOUND` |
| Five access masks | `1`, `3`, `0x20019`, `0x20119`, `0x20219` |
| Open-call total | 4 execution views x 2 repetitions x 8 names x 5 masks = 320 |
| Raw value records | 4 execution views x 3 registry views x 10 values = 120 |
| Owned key handles | closed; process handle counts unchanged per execution view |

The guest preserves the established read-only snapshot contract. A successful
native open requesting `KEY_SET_VALUE` does not authorize guest mutation:
access mask `3` is denied for present guest keys. Likewise, a native missing
suffix name remains explicitly unselected in the guest. The replay tests all
320 recorded inputs for each of the three guest ABIs and asserts these two
policy boundaries separately. It also queries every raw value through
`NtQueryValueKey`, including the default value, with exact type, length, bytes
and returned-size checks. No universal native equivalence is claimed for
write access or names outside the selected namespace.

Microsoft documents the owning interfaces in
[RegOpenKeyExW](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regopenkeyexw),
[RegEnumValueW](https://learn.microsoft.com/en-us/windows/win32/api/winreg/nf-winreg-regenumvaluew)
and [Nt/ZwOpenKey](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-zwopenkey).
The private key's contents and individual status/path observations above come
from the captured native producer, rather than a public API guarantee.

## Source and verification boundaries

The frozen source graph contains 26,902 inputs, SHA-256
`3a026ef233ba5737936b83e56c6d461e4d59386c3c44b185054db98d22a3f186`.
Its 11 overrides are seven Rust files and four compiled native-log fixtures.
Publication receipts added later are outside that compilation graph. One
native snapshot-selection test fails against published parent `9e3e0647c`;
the original 1,422 B test prefix is unchanged in the final test file. Original
baseline setup failures (wrong source-directory selection and insufficient
virtual-disk space) remain separate from the completed red run.

The native proof checker validates source/executable/DLL identities, PE
architectures, completed stages, all 320 open inputs, all 120 raw value records,
view parity and handle balance. Five semantic corruptions are rejected after
repairing transitive log/launch hashes; each negative has a fresh positive
restoration. Windows live mode rehashes the actual producer source, four
executables and eight DLL inputs.

Retained Assist ABI184 owning consumers and their archives are a distinct gate
from standalone Cargo tests and current Assist ABI274 packages. Both actual
owning routes now advance whoami to `NtQueryInformationProcess` class 52.
Rust terminal index 2,802 corresponds to 2,803 C API turns. C API reason 5 and
an exit-code field of zero denote emulator failure. Both smoke routes exit
zero. The diagnostic request has current-process pseudo-handle `-1`, selector
11, length 8 B and null ReturnLength; its implementation belongs to the next
increment.

| Host, no default features | Full library | C API | Integration |
|---|---|---|---|
| macOS ARM64 | 8,153 pass; zero failures; two existing ignores | 183 pass | 544 pass |
| Linux AMD64 container on ARM64 host | 8,147 pass; zero failures; two existing ignores | 183 pass | 544 pass |
| Native Windows ARM64 | 7,619 pass; five existing lowerer failures; two existing ignores | 183 pass | four pass |

All-target checks on all three hosts, both Windows cross targets and the
repository's canonical Linux all-target/default-feature Clippy command pass.
The extra `-D warnings` experiment rejected an unchanged AVX-512 test helper;
its transcript is retained separately, without changing the canonical warning
policy or suppressing diagnostics. The five native Windows failures are exactly
the previously recorded ARM64-lowerer set, rather than unclassified new failures.

Actual retained Assist consumers pass on all three hosts: eight selected-carrier
link rules and seven CTests on macOS, plus five compiled/run C++ consumers on
Linux and Windows. Complete archive walks and before/after hashes bind their
core/carrier pairs to 3,875 selected inputs. The new fixed NLS path is also
present in the actual macOS core archive. Six declared public compiler
projections retain original identities while withholding private Assist
compiler diagnostics. Captured CRLF/progress records remain unchanged.

The process-query frontier, independent image-view unwind-registration gap
and previous image/runtime policy boundaries keep the full same-host userland
objective active. RAX C ABI 1.14.0, current Assist Rust ABI minor 274,
dependencies, build defaults, permissions and CI definitions are unchanged.
No whole-current-root GUI/package result is inferred from retained ABI184
consumers. No changelog or release work is included.
