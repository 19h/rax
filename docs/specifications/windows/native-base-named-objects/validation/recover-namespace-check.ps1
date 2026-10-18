$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010';$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
$env:RUSTC=$b+'\toolchain\bin\rustc.exe'
$env:CARGO_HOME=$b+'\cargo-home'
$env:CARGO_TARGET_DIR=$b+'\target-system-time-final'
$env:CARGO_BUILD_JOBS='2'
$env:CARGO_INCREMENTAL='0'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location ($b+'\src-base-named-objects')
& cmd.exe /d /c ('cargo.exe check --locked --offline --no-default-features --all-targets > '+$b+'\namespace-recovery-all-targets.log 2>&1')
$code=$LASTEXITCODE
Copy-Item ($b+'\namespace-recovery-all-targets.log') ($s+'\native-base-final-all-targets.log')
exit $code
