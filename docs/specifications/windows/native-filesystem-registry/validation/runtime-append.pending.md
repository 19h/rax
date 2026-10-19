
### Fixed FileSystem registry root (2026-10-19)

Native Windows runtime selection captures the fixed HKLM System FileSystem
subtree with the existing complete bounded immutable tree adapter. Root-only
not-found establishes absence; errors and partial capture fail. Shared registry
views, raw values, read-only grants, aggregate limits and descendant lifetime
remain unchanged; guest paths and native handles are never forwarded.
The 320 native opens across four ABI profiles on one ARM64 Windows kernel,
six shared/nine Windows tests, full three-host gates, retained consumers,
assumptions and portable evidence are recorded in
`docs/specifications/windows/native-filesystem-registry/`.

The actual retained-owner core is ACTUAL_CORE_BYTES bytes, SHA-256
`ACTUAL_CORE_SHA`. Both ordinary guests open FileSystem successfully and
advance 8,728 turns to unsupported `NtSetInformationWorkerFactory` class 13.
Neither emulated guest has demonstrated successful exit; the full process
objective remains incomplete. Retained Assist ABI 184 consumer validation
does not establish a full current GUI/plugin/package build.
