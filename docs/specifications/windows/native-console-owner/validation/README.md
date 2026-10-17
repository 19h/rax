# Validation evidence

`check.py` verifies completed records collected by
`collect-console-owner-validation.py`. Collection requires all selected core
and owning drivers to have terminated with their specified outcomes. It rejects
missing gates, unexpected failures, changed frozen inputs and replacement of
earlier evidence. Running the checker before collection reports missing records.

The two source epochs remain distinct. Epoch 1 retains the initial synthetic
WoW64 stack assertion failure. Epoch 2 corrects the synthetic transition's
4-byte return-address pop and the setter's NULL capture priority. The installed
NTDLL return instructions retain separate strict stack checks.

The initial native Windows job builds epoch 1 and performs its ordinary startup
observations before its focused tests. The observation helper loaded the retained
detached diagnostic variant before a subsequent replacement was written. The
replacement did not run. A separate native continuation verifies the original
PID remains live and waits for its actual terminal result before starting the
unchanged corrected core and owning gates. The continuation records the SHA-256
of the handoff module loaded at startup. An initial 101 in this path represents
the retained focused failures; the checker requires their exact identities.

A retained alternative handoff can explicitly record epoch 1 focused tests as
unrun. Only that explicit ledger value permits omission of an initial native
focused result. The checker never infers nonexecution from a handoff file's
existence. Final epoch 2 selections and assertions remain unchanged.

The selected checks comprise unfiltered core libraries, focused service tests,
prior process-query regressions, all-target compilation, Windows integration
tests, C API tests and `rax-user` builds. Unix `user_windows_memory` selection is
compile-only with zero runnable tests; its four positive native cases require
Windows. A complete native library record must preserve and identify the five
existing AArch64 lowerer failures rather than claim an all-tests-pass result.

Owning-product records describe retained Assist carriers. They bind 309 selected
RAX inputs, actual empty-feature core artifacts, and complete static-archive
member walks. The frozen Assist public ABI minor is 184; the compared published
root's minor is 186. Selected current C++ consumers and the RAX 1.11.0 interface
are checked separately. These results do not establish a whole-current-root,
plugin, GUI or package validation result.

Ordinary program observations use the unchanged observer and production process
parameters. Controller exit 0, fixture exit 77, controlled parameter mutation,
and exhaustion of an observation budget do not establish ordinary guest entry
or successful guest exit. Inspect the actual terminal outcome in each raw log.

After collection, run from the RAX root:

```text
python3 -B docs/specifications/windows/native-console-owner/validation/check.py
```

The checker rejects Python optimization. `results.json` binds each collected
file's byte count and SHA-256 digest. Source snapshots and selected input
manifests are independently compared with the published source. Native oracle
provenance is checked by the parent corpus checker before validation records.
