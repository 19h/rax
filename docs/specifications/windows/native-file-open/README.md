# Native existing-file opens

`NtOpenFile` captures guest arguments, opens an existing authorized file or
directory, and publishes an owned guest handle. Selected installed DLLs retain
read-only library authority independently of general `host_filesystem` access.
Unselected files and write/delete requests do not acquire that authority.
`NtCreateFile`, file-ID opens and file-backed `NtCreateSection` remain separate
implementation frontiers. The full same-host process-userland objective remains
active.

The observed ordinary request opens `\??\C:\WINDOWS\SYSTEM32\VERSION.dll` with
desired access `0x00100021`, sharing `0x5`, options `0x60` and object attributes
`0x840`. Actual owning-core and Assist C++/C API runs now complete that open.
The smoke program exits with status zero. `whoami.exe` advances to a file-backed
`NtCreateSection` request with access `0xD`, protection `0x10`, allocation flags
`0x01000000`, no maximum-size pointer, and the retained file handle. Its explicit
unsupported result is not successful program termination, even though the C API
result's separate exit-code field is zero.

| Observed caller effect | Native ARM64/x64 | WoW64 x86/x86-LAA |
| --- | --- | --- |
| Handle width | 8 bytes | 4 bytes |
| Successful IO_STATUS_BLOCK | 16 bytes | 8 bytes |
| Initial output probing | Full handle width, first 4 IOSB bytes | Wrapper capture precedes kernel work |
| Late IOSB copyback fault | Published handle remains valid | Temporary handle closes; published bytes can remain |
| Invalid retained root with faulty name characters | Native capture order | Headers captured; redirection does not capture characters |

Syntactic access/share/options checks precede native output probing. Native
initial probes and late caller stores have distinct guard-page effects. Each
pointer-sized output store is atomic at its own width. Successful information
is `FILE_OPENED = 1`. Independent output-prefix, guard and dual-fault captures
own these details; documentation of the structure alone does not establish
fault order.

Windows opens use host-local NT structures and real owned file handles. Guest
pointers, security descriptors and inheritance are not forwarded to the host.
The granted access mask is queried from the retained object. Win32 and native
opens share a volume-plus-128-bit file identity, including hard-link aliases.
Win32 host reservations retain the requested DELETE grant. A regression against
candidate1 failed because a rejected delete-on-close attempt deleted its fixture
after the final close; exactly the final candidate2 test then passed. The native
control covers zero, DELETE, GENERIC_READ|DELETE and GENERIC_WRITE|DELETE held
grants, using read/write sharing without delete sharing.

On macOS and Linux, retained directory opens and attribute queries traverse
with descriptor-relative operations. Directory rename and replacement do not
substitute the old pathname as lookup authority. Case-insensitive fallback
enumerates directory entries and refuses ambiguous matches. No-reparse traversal
uses no-follow operations for each component. Linux `/proc/self/fd` and macOS
`F_GETPATH` provide bookkeeping spelling; retained directory traversal still
works when that spelling is unavailable. Reopening a retained regular file
checks its device/inode identity. External host namespace mutation has no
general atomicity guarantee.

Name/security capture takes `O(U + S)` time and space for `U` counted name bytes
and `S` captured security bytes. Sharing validation visits `H` live process
objects, taking `O(H)` time. Directory fallback costs the total enumerated name
bytes across traversed components. Output serialization uses at most
`8 + 16 = 24` bytes on native64 and `4 + 8 = 12` bytes on WoW64. Filesystem and
kernel latency are not assigned a constant bound.

Native producer v10 executes `459 * 4 = 1836` calls across four ABI views on one
Windows 10.0.29683 ARM64 kernel. Sharing-v2 adds `4 * 4 = 16` calls. The compiled
pair corpus executes `210 * 3 = 630` dispatcher calls per host; its 54 boundary
roles add `54 * 3 = 162`. Native v6 is rejected for a producer indexing error.
V9 roles 426/427 used an invalid protection combination and are excluded from
protected-memory evidence. The compiled boundary set uses roles 404..425 and
428..459. No independent physical x64-kernel equivalence is claimed.

The frozen candidate2 ledger covers 26,848 inputs / 2,021,965,191 bytes.
Windows replaces only the separately pinned Cargo configuration:
`2021965191 - 552 + 374 = 2021965013` bytes. Both original and native configuration
identities are checked. New publication records are added after the candidate
freeze and are not compilation inputs. Existing frozen files remain identical.
Final counts and exact selections belong to the complete gate receipts. Five
established Windows lowerer failures remain visible in the unfiltered suite;
they are neither skipped nor converted into passes.

The first candidate2 full Windows attempt reused the pre-fix unit-test image
after the baseline producer used the same Cargo target. Its dependency record
contains the baseline source root; it is rejected as final-candidate evidence.
The replacement run evicts only the task-owned primary-crate artifacts, rebuilds
the frozen candidate, selects the emitted Cargo library-test executable and
hashes that exact image around both focused and unfiltered executions. The
rejected image is retained independently. Source fingerprints alone did not
establish which compiled image executed.

Final candidate2 executions:

| Host | Library passed | Failed | Ignored | C API passed | Integration passed |
| --- | ---: | ---: | ---: | ---: | ---: |
| macOS | 8128 | 0 | 2 | 183 | 544 |
| Linux | 8122 | 0 | 2 | 183 | 544 |
| Windows | 7590 | 5 established lowerer failures | 2 | 183 | 4 |

All 26 focused cases and all-target checks pass on each host. Separate Windows
ARM64/x64 checks establish compilation. Eight oracle corruptions and eight
supplemental owning-record corruptions per host are rejected, with a fresh
positive restoration after each. Supplemental round2 keeps before/after records
consistent when testing omitted source inputs and incomplete archive walks;
rejection reaches the intended semantic check.

Owning evidence selects the actual Cargo core and Assist carrier, records their
feature sets, walks every archive member, runs linked consumers, and compares
identities before/after. These are retained Assist ABI184 consumers, not the
entire current ABI274 GUI/plugin/package surface. Four independent retained byte
copies preserve the exercised Windows core, carrier and two ordinary callers.
Native Windows audits rehash those copies; other hosts explicitly report that
native-file byte check as unperformed.

Public Cargo streams include artifact/build metadata only. Private compiler
diagnostics and the private CMake compiler transcript remain local.
`validation/public-projections.json` binds each exact published byte stream to
its original checkpoint identity. Other bytes cannot resolve to that identity.
Complete test transcripts are retained separately. No history rewriting or
publication of installed system binaries is involved.

Run `python3 -B check_validation.py --source-root <candidate-source-root>` to
audit the complete gate selections, frozen source, independent native oracles,
actual owning archives, ordinary-program results and red/green regression.
`assumptions.json` records the Assumption Register and bounded findings.
Rejected loader/controller attempts and candidate1 failures are historical
evidence, not candidate2 validation.

Primary contracts, accessed 2026-10-21:
[NtOpenFile/ZwOpenFile](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-zwopenfile),
[IO_STATUS_BLOCK](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/ns-wdm-_io_status_block),
[FILE_ID_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info).
ABI fault ordering and grant-dependent query behavior are measured native
observations rather than inferences from those contracts.
