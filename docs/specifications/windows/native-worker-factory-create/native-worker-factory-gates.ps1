$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010';$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH;$env:RUSTC=$b+'\toolchain\bin\rustc.exe';$env:CARGO_HOME=$b+'\cargo-home';$env:CARGO_TARGET_DIR=$b+'\target-io-completion-tests'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location ($b+'\src')
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features --lib worker_factory -- --nocapture > '+$b+'\native-worker-factory-targeted.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-worker-factory-targeted.log') -Destination ($s+'\native-worker-factory-targeted.log')
Write-Output ('gate targeted exit='+$gateExit)
if($gateExit -ne 0){exit $gateExit}
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features --lib event_ -- --nocapture > '+$b+'\native-worker-factory-events.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-worker-factory-events.log') -Destination ($s+'\native-worker-factory-events.log')
Write-Output ('gate events exit='+$gateExit)
if($gateExit -ne 0){exit $gateExit}
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features --lib io_completion -- --nocapture > '+$b+'\native-worker-factory-io-completion.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-worker-factory-io-completion.log') -Destination ($s+'\native-worker-factory-io-completion.log')
Write-Output ('gate io-completion exit='+$gateExit)
if($gateExit -ne 0){exit $gateExit}
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features --lib  -- --nocapture > '+$b+'\native-worker-factory-full.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-worker-factory-full.log') -Destination ($s+'\native-worker-factory-full.log')
Write-Output ('gate full exit='+$gateExit)
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features -p rax-capi -- --nocapture > '+$b+'\native-worker-factory-isolated-capi.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-worker-factory-isolated-capi.log') -Destination ($s+'\native-worker-factory-isolated-capi.log')
Write-Output ('gate capi exit='+$gateExit)
if($gateExit -ne 0){exit $gateExit}
& cmd.exe /d /c ('cargo.exe build --locked --offline --no-default-features --all-targets > '+$b+'\native-worker-factory-isolated-all-targets.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-worker-factory-isolated-all-targets.log') -Destination ($s+'\native-worker-factory-isolated-all-targets.log')
Write-Output ('gate all-targets exit='+$gateExit)
if($gateExit -ne 0){exit $gateExit}
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features --test user_windows_memory -- --nocapture > '+$b+'\native-worker-factory-isolated-integration.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-worker-factory-isolated-integration.log') -Destination ($s+'\native-worker-factory-isolated-integration.log')
Write-Output ('gate integration exit='+$gateExit)
if($gateExit -ne 0){exit $gateExit}
exit 0
