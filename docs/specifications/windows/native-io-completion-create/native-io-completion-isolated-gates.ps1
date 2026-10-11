$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010';$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH;$env:RUSTC=$b+'\toolchain\bin\rustc.exe';$env:CARGO_HOME=$b+'\cargo-home';$env:CARGO_TARGET_DIR=$b+'\target-io-completion-tests'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location ($b+'\src')
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features -p rax-capi -- --nocapture > '+$b+'\native-io-completion-isolated-capi.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-io-completion-isolated-capi.log') -Destination ($s+'\native-io-completion-isolated-capi.log')
Write-Output ('gate capi exit='+$gateExit)
if($gateExit -ne 0){exit $gateExit}
& cmd.exe /d /c ('cargo.exe build --locked --offline --no-default-features --all-targets > '+$b+'\native-io-completion-isolated-all-targets.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-io-completion-isolated-all-targets.log') -Destination ($s+'\native-io-completion-isolated-all-targets.log')
Write-Output ('gate all-targets exit='+$gateExit)
if($gateExit -ne 0){exit $gateExit}
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features --test user_windows_memory -- --nocapture > '+$b+'\native-io-completion-isolated-integration.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-io-completion-isolated-integration.log') -Destination ($s+'\native-io-completion-isolated-integration.log')
Write-Output ('gate integration exit='+$gateExit)
if($gateExit -ne 0){exit $gateExit}
exit 0
