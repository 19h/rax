# Owned LPC section-view foundation

This corpus records 30 successful private same-process LPC controls on ARM64
Windows 10.0.29683.1000. Native-width 48-byte client views were measured with
ARM64, x64, x86 and large-address-aware x86 producers (20 cases). True 24-byte
client views were separately measured with both x86 profiles (10 cases).
These are compatibility ABIs on an ARM64 kernel, not physical x64 kernel proof.

The controls show real bidirectional section aliases and endpoint-local view
retirement. Closing the section handle preserves both views. The last local
port handle releases only its local view; a duplicate delays that release.
Explicit unmap followed by private allocation reuse preserves the replacement
on subsequent port close. The 30 cases close 126 handles, release 60 views,
perform 12 manual unmaps and preserve/free six private replacements.

The Rust replay compares local VM states, byte aliases and handle lifetimes
with every retained native trace. It does not replay connection handshakes,
message replies, remote-process address spaces, biased native PointerCount,
or original CSR startup. A separate model stress case checks mapped-address
reuse; there is no native mapped-replacement claim. Protection beyond ordinary
NOACCESS/READONLY/READWRITE is explicitly unsupported for these new views.

The model retains actual SharedObject backing. Aliases of one retained Arc
charge each object page once; private allocations remain separately charged.
At release, only pages losing their final alias return commitment. Distinct
allocation tokens prevent delayed endpoint cleanup from releasing a replacement.
Automatic address placement costs O(P log R + M log M) time and O(P + M)
additional metadata/scratch for P mapped object pages, R charged pages and M
existing allocation/mapping runs. Explicit placement avoids the M-run sort.

Run `python3 docs/specifications/windows/native-lpc-owned-views/check.py` for
source/raw-log identities and native cleanup totals. Native identities were
independently collected from the actual guest producer source, PE headers,
executables and output before copying these raw bytes. Executable identities
are evidence records, not binaries included in this corpus. Platform compile,
execution and owning-archive results must be recorded separately after running.

This foundation does not advance the original Windows startup frontier.
