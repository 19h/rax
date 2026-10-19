# Installed FileSystem registry snapshot

Native Windows runtime selection additionally captures the fixed
`\Registry\Machine\System\CurrentControlSet\Control\FileSystem` subtree.
`TreeRoot::FileSystem` uses the existing read-only HKLM acquisition adapter and
complete immutable tree representation. No guest path reaches a host registry
operation, and guest handles contain no host HKEY. A present root retains every
bounded descendant/value; only root `ERROR_FILE_NOT_FOUND` establishes absence.
Other root errors, partial enumeration, changed metadata, or an enumerated
child disappearing fail acquisition.

The root is 61 UTF-16 code units, or 122 bytes, excluding a terminator. The
observed guest adds a trailing separator, producing 62 code units/124 bytes.
Existing ordinal case folding, literal dot/slash names, relative opens, raw
value serialization, read-only guest grants, and reference-counted descendant
lifetime remain unchanged. A sibling prefix and unselected parent remain
outside the selection. The sixth fixed tree and the primary-user policy tree
share the existing aggregate limit of 4,096 values and 16 MiB of value-name/data
storage. Per-tree bounds remain 1,024 keys, depth 32, and 1 MiB of stored paths.
Native capture and lookup reuse the existing algorithms; no new host callback,
guest syscall, public ABI, dependency pin, or default is introduced.

The independent C++ producer records 320 opens: four native ABI profiles,
two samples, eight path variants, and five access/view masks. ARM64, x64
compatibility, x86, and x86-LAA all open the root, its trailing-separator form,
and its lowercase form successfully; the recorded missing/sibling/dot paths
return `STATUS_OBJECT_NAME_NOT_FOUND`. Source, PE machine/LAA, exit status,
six native DLL identities, and balanced handle counts are checked. The four
profiles share one physical ARM64 Windows kernel. Twelve rebound logical
corruptions are rejected by the independent checker. Stored Microsoft primary
provenance describes the inherited shared registry views.

The tests-only native baseline compiles and fails on the missing selected key
before implementation. Its final test name gains `registry` for a unique
focus filter; the original assertion body is verified unchanged. Six shared
tests exercise root/sibling boundaries, raw values, lifetime, duplicate rejection,
the aggregate budget across every previous root including the primary-user
policy, and all guest ABIs. Three additional Windows tests compare the real
FileSystem snapshot with independent native opens/values in all three registry
views, distinguish root absence from other errors, and exercise complete native
traversal plus child disappearance using a controlled IFEO fixture. The
fixture's 54 keys/106 values/4,756 bytes describe IFEO acquisition mechanics,
not the host's FileSystem contents.

The complete source epoch binds 23,843 files and 1,716,662,603 bytes. Full
three-host gates, C API/integration/all-target checks, and retained Assist
consumers are recorded under `validation/`. Public RAX C ABI 1.14.0 is unchanged.
Retained Assist Rust ABI 184 and 538 selected source inputs are distinct from
a complete build of current root GUI/plugin/packages. The actual retained-owner
core is 503848220 bytes, SHA-256 `456955a2b903926620df54ccfdbbd3406f08683611d18b83ee41bb18db88d81c`.

Both ordinary native-library guests open FileSystem successfully at their
previous terminal turns and advance another 8,728 normally executed observer
turns to unsupported `NtSetInformationWorkerFactory` information class 13.
Native controls exit successfully. Neither emulated guest has demonstrated
successful exit, and controller exit status zero is not guest success.
The full same-host process-emulation objective remains active.

The executed Unix driver retained the previous group's artifact prefix.
Original records/logs are preserved alongside FileSystem filename aliases.
The promotion record and verifier prove that only artifact basenames changed;
commands, test selections, result fields, exit codes, and log bytes are identical.
This artifact naming issue is separate from product source and test behavior.

Assumptions, falsification probes, change surfaces, bounded findings, exact
known Windows failures, quality gates, negative controls, and portable
verification receipt scope remain under `validation/`. Run:

```text
python3 -B docs/specifications/windows/native-filesystem-registry/check_native.py
python3 -B docs/specifications/windows/native-filesystem-registry/check_validation.py .
```
