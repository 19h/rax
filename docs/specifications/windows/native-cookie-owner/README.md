# Installed-loader security-cookie ownership

This change preserves mapped `/GS` cookie bytes when an installed Windows
runtime owns initialization. The built-in DLL path still initializes a default
cookie. Both paths retain the existing load-configuration and cookie-span
validation. No host token, process handle, permission, ABI, dependency or default
feature changes.

The initial installed ARM64 NTDLL trace first returned `0xC000007B` at
`0x1800DA078`, turn 102175. Its preceding instructions compared the main image's
cookie with `0x00002B992DDFA232`, returned zero for the preinitialized value,
and rejected the entry-bearing image with subsystem major version 10. Private
PDB function labels are inferences: the PDB information-stream age differs from
the image age. The recorded instruction bytes and register values are the
direct evidence.

Four independently inspected native producer profiles each create four owned
suspended children. ARM64 and x64 use 8-byte cookies; x86 and x86-LAA use 4-byte
cookies. All run on Windows 10.0.29683 on one ARM64 kernel.

| Suspended child condition | Native result |
| --- | --- |
| Leave the file-default cookie intact | Child reaches its entry and exits zero |
| Explicitly restore that same default | Child reaches its entry and exits zero |
| Write a nondefault cookie before the first instruction | `0xC000007B`, no child entry report |
| Write zero before the first instruction | `0xC000007B`, no child entry report |

Defaults are `0xBB40E64E` for PE32 and `0x00002B992DDFA232` for PE32+.
Successful native children report a changed cookie after loader/CRT startup.
This observation does not distinguish every contribution of the loader and
CRT. A separate read-only suspended-child probe verifies that both the
executable and mapped native ARM64 NTDLL retain their file-default cookie before
the initial thread runs. Its first, failed attempt and corrected zero-address
handling are retained separately.

The fixed implementation reads the same validated 4/8-byte cookie span as
before, then defers mutation whenever an actual selected native runtime exists.
It does not use the requested configuration flag as a substitute for runtime
presence. The shared initializer covers executable and DLL mapping. Its new
branch uses constant auxiliary space and performs no allocation or entropy
advance; existing fixed-size load-configuration reads remain unchanged.

The three portable regressions exercise x86, x64 and ARM64 guest state:
default/custom/zero/full-width preservation and unchanged entropy in native
mode; retained built-in initialization and existing-value behavior; and
unchanged rejection of foreign cookie pointers. Their native metadata fixture
does not acquire host DLLs and is not proof of installed-loader execution.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| K1 | Installed RTL owns initialization after suspended creation. | Sixteen native child observations. | Preserve file cookie bytes in native mode. | Four producer profiles, default/preseed/zero controls and installed startup. | A suspended preinitialized main cookie or successful preseed control. | Confirmed for the recorded build and profiles. |
| K2 | Native ARM64 NTDLL starts with its file-default cookie. | Independent suspended mapping read. | No NTDLL preseed exception. | Compare executable and actual NTDLL mappings before first resume. | A differing suspended NTDLL cookie. | Confirmed for recorded ARM64; other native mappings unknown. |
| K3 | Built-in initialization and malformed-span admission are retained. | Shared initializer and focused regressions. | No built-in behavior change. | All three guest ABIs, custom/default/zero/full-width values, foreign pointers and entropy state. | A changed built-in result, admitted foreign pointer or native entropy advance. | Confirmed by final tests. |
| K4 | Frozen Assist carriers exercise the same RAX 1.11.0 contract and selected current C++ consumers. | Three-host Cargo artifacts, source hashes and complete archives. | Scoped embedding validation. | Empty core features, 310 source inputs, nine current C++ source comparisons. | A differing selected input or archive member failure. | Confirmed within ABI-minor184 carrier; whole-current-root validation unknown. |

The change affects loader initialization and the corresponding process-start
behavior. It does not change decoding, CPU instructions, SMIR, optimization,
JIT, devices, syscall schemas or C ABI layouts. Windows, macOS and Linux compile
the same loader branch; installed DLL acquisition remains a Windows-host
capability. Assist plugin/UI, conversation, providers, prompts, permissions,
main-thread dispatch, transport, persistence and update logic have no code
changes. RAX pin/engineering records and the embedding validation are affected.

Bounded findings:

- High: ordinary Windows process completion remains unproven. The fixed native
  ARM64 smoke and `whoami.exe` runs now stop at unsupported
  `NtQueryInformationToken` class 29, using current-process-token pseudo-handle
  `-4`. This remains a blocker for the full process-userland objective.
- Medium: the five existing native Windows AArch64 lowerer failures are
  retained, not converted to skips or passes.
- Medium: initial broad Unix attempts exposed timing-sensitive unrelated tests.
  The failed logs and interrupted environment attempts are retained. Final
  serial executions use the same assertions and test selection.
- Medium: the retained Assist carrier uses public ABI minor 184, while the
  compared root uses 186. Scoped embedding proof does not establish a complete
  current-root, plugin, GUI or package result.

Final Rust1.95.0 library results:

| Host | Passed | Failed | Ignored | Filtered |
| --- | ---: | ---: | ---: | ---: |
| macOS | 7839 | 0 | 2 | 0 |
| Linux | 7833 | 0 | 2 | 0 |
| Windows ARM64 | 7263 | 5 | 2 | 0 |

The five native Windows failures are the unchanged four BZHI and one FP16
lowerer tests, checked by exact test identity. C API168 passes on every host;
all-target compilation, Unix Windows integration544 and native memory4 pass.
The new cookie tests pass on all hosts, and all seven existing owner-service
checks, including actual installed NTDLL leaves, pass on Windows. Final serial
Unix runs preserve every test assertion; initial unrelated timing failures and
an interrupted environment attempt are retained independently.

The first Windows embedding attempt found a missing task-owned RAX junction.
The next found missing dependency-cache source files. The final attempt restores
10,001 missing files using their existing vendored SHA-256 values, retains both
Cargo configurations byte-for-byte, and reruns only the embedding checks. No
repository dependency pin is changed. All three scoped carriers bind the same
310 source inputs and empty RAX features to two structurally walked archives.
macOS also passes seven selected CTests and both static-text scans; Linux and
Windows pass the five named C++ consumers. This is the retained carrier boundary
described above, not a complete current Assist build.

`check.py` verifies native evidence and frozen implementation bytes.
`validation-check.py` additionally verifies raw final counts, exact failure
identities, failed attempts, source/feature bindings and the complete archive
records. The two ordinary installed-runtime observations stop at
`NtQueryInformationToken` class29: smoke at turn2130489 and `whoami.exe` at
turn2132851, both at `0x180001220`. Neither is an application-success result.
No native executable or library is stored in this corpus.
