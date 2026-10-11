$ErrorActionPreference='Stop'; $b='C:\Windows\Temp\assist-native-rax-20261010'; $s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH; $env:RUSTC=$b+'\toolchain\bin\rustc.exe'; $env:CARGO_HOME=$b+'\cargo-home'; $env:CARGO_TARGET_DIR=$b+'\target'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& ($b+'\python\python.exe') ($s+'\native-io-completion-current-overlay.py')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
Set-Location ($b+'\src')
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features --lib io_completion -- --nocapture > '+$b+'\native-io-completion-final-targeted.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-io-completion-final-targeted.log') -Destination ($s+'\native-io-completion-final-targeted.log')
Write-Output ('gate targeted exit='+$gateExit+' sha256='+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-io-completion-final-targeted.log')).Hash)
if($gateExit -ne 0){exit $gateExit}
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features --lib event_ -- --nocapture > '+$b+'\native-io-completion-final-events.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-io-completion-final-events.log') -Destination ($s+'\native-io-completion-final-events.log')
Write-Output ('gate events exit='+$gateExit)
if($gateExit -ne 0){exit $gateExit}
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features --lib -- --nocapture > '+$b+'\native-io-completion-final-full.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-io-completion-final-full.log') -Destination ($s+'\native-io-completion-final-full.log')
Write-Output ('gate full exit='+$gateExit+' sha256='+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-io-completion-final-full.log')).Hash)
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features -p rax-capi -- --nocapture > '+$b+'\native-io-completion-final-capi.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-io-completion-final-capi.log') -Destination ($s+'\native-io-completion-final-capi.log')
Write-Output ('gate capi exit='+$gateExit+' sha256='+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-io-completion-final-capi.log')).Hash)
if($gateExit -ne 0){exit $gateExit}
& cmd.exe /d /c ('cargo.exe build --locked --offline --no-default-features --all-targets > '+$b+'\native-io-completion-final-all-targets.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-io-completion-final-all-targets.log') -Destination ($s+'\native-io-completion-final-all-targets.log')
Write-Output ('gate all-targets exit='+$gateExit+' sha256='+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-io-completion-final-all-targets.log')).Hash)
if($gateExit -ne 0){exit $gateExit}
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features --test user_windows_memory -- --nocapture > '+$b+'\native-io-completion-final-integration.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-io-completion-final-integration.log') -Destination ($s+'\native-io-completion-final-integration.log')
Write-Output ('gate integration exit='+$gateExit+' sha256='+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-io-completion-final-integration.log')).Hash)
if($gateExit -ne 0){exit $gateExit}
exit 0
