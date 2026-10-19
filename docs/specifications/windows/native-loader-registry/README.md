# Fixed native loader registry snapshots

Installed Windows runtime selection now captures three additional fixed registry
roots through the existing complete-or-known-absent immutable tree adapter.
These roots were observed in ordinary execution, in this order:

| Native root below `\Registry\Machine` | Observed desired access | Captured native root |
| --- | --- | --- |
| `System\CurrentControlSet\Control\SafeBoot\Option` | `0x3` | absent |
| `System\CurrentControlSet\Control\Srp\GP\DLL` | `0x20019` | absent |
| `Software\Policies\Microsoft\Windows\Safer\CodeIdentifiers` | `0x1` | present |

The existing IFEO and StateSeparation roots remain selected. Root-only Win32
`ERROR_FILE_NOT_FOUND` establishes absence; other acquisition errors, unstable
values or disappearing enumerated children abort capture. No guest path reaches
a native registry opener. Unknown parents and siblings retain the unsupported
selection boundary. Present trees own raw typed value bytes and descendants;
guest handles retain that ownership after the catalog is dropped.

The inherited guest registry contract is read-only. Write-intent access to a
present tree is denied even when the captured host token could write it. Native
Safer access `0x3` succeeds on this subject; that observation is retained as a
reference and does not broaden guest permissions. The ordinary Safer request
uses `0x1`. Both observed SYSTEM requests reach known absence before grants.
This change does not establish general mutable registry parity.

Tree metadata limits remain1024keys,maximum descendant depth32(rootdepth0),
255UTF16units per child component,
32767UTF16units per path and1048576aggregate path bytes per selected root.
Existing value count/byte limits remain shared across all selected roots.
For Kkeys,Vvalues,Bcopied bytes, construction uses
O((K+V+B)log(K+V+2)) local work and O(K+V+B) retained storage,
excluding native API internal costs; the number of selected fixed roots is5.
Existing three-attempt per-key stability checks and ordinal case folding remain
unchanged. Registry names use their own separator grammar, independent of host
filesystem paths. The [Microsoft registry view contract](https://learn.microsoft.com/en-us/windows/win32/winprog64/shared-registry-keys)
is stored with retrieval time and SHA256; native view probes independently agree.

## Native reference and validation

Three producers record2samples*8paths*5rights*4ABIprofiles=960cases. Each profile
contains80cases per root.840observations address the selected roots and120are
outside-prefix controls. SafeBoot and SRP each return320name-not-found results;
Safer returns120successful root/alias opens and200missing descendant/control
results. Success handles are opaque and nonzero, never numeric goldens.
Native source, PE machine/LAA status, process handle counts and six installed
runtime module identities are checked before/after. The four ABI profiles
(ARM64,x64compatibility,x86,x86LAA) share one Windows10.0.29683 ARM64 kernel.
The native checker rejects18corrupted copies.

| Host | Unfiltered core passed/failed/ignored | New focused tests | C API | Process integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 8008/0/2 | 18 | 183 | 544 plus0cfg cases |
| Linux x86-64 container | 8002/0/2 | 18 | 183 | 544 plus0cfg cases |
| Windows ARM64 | 7461/5/2 | 27 | 183 | 0cfg cases plus4native |

All three all-target checks pass. The five Windows lowerer failures match the
exact established names in `validation/windows-existing-failures.json`.
A tests-only baseline compiles, then fails its actual native absence assertion
on the prior implementation (0passed/1failed). All Rust changes preceded the
final full gates. The complete source epoch binds23283inputs/1657974640bytes.
The post-gate runtime engineering append is separately identified.

Actual retained Assist core/carrier builds and selected C++ consumers pass on
all three hosts, binding528selected source inputs and full archive walks before
and after C++ links. macOS also passes seven exact CTests and actual CLI/MCP
carrier linkage/protected-text checks. The retained Assist Rust ABI is184;
eight C++ inputs were previously compared with root ABI264. These results do
not establish whole-current-root GUI/plugin/package coverage. Public RAX C API
remains1.14.0; defaults, dependencies and public layouts are unchanged.

## Ordinary execution and limits

The smoke guest passes the old turn2883900 and reaches2883974; whoami passes
2886185 and reaches2886259. Both execute another74observer turns normally,
return C0000034 for both absent SYSTEM roots, successfully open Safer, and stop
at unsupported NtQueryInformationToken class1. Neither guest exits successfully.
Both native controls exit0; native whoami emits `nt authority\system` followed
by CRLF. Guest stdout/stderr remain empty at this startup frontier.

The exact validated retained-owner core is503515446bytes, SHA256
`1e1a94d1cd59c4225dd1d85f1391083fdac087c2dcf73a5ea55b24801e170c18`. The observer links this actual archive, executes the prefix
normally and binds source/executable/program/six-module/archive identities
before and after. Controller success is distinct from guest success.

`validation/loader-plan.json` and `validation/final-assumptions.json` own the
final scope. Earlier SafeBoot-only and two-root plans/candidates are historical
exploration evidence. The first final native driver failed before invoking a
compiler because its preparation-helper name was stale; the separately named
epoch3b driver verifies the already materialized immutable source and completes
the full gates. The Safer controlled-present test has an inherited srp_dll
diagnostic label; its actual test identity, selected root and assertions are
Safer. No inference relies on that diagnostic text.

Run `python3 -B check_native.py .` here and
`python3 -B check_validation.py /path/to/rax` from any supported host.
The evidence manifest binds every evidence file except itself. Three-host
verification receipts record their original checked-envelope scope; later
receipts and the verbatim whitespace audit are explicitly added afterward.
The complete same-host process objective remains incomplete.
