$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH;$env:RUSTC=$b+'\toolchain\bin\rustc.exe';$env:CARGO_HOME=$b+'\cargo-home';$env:CARGO_TARGET_DIR=$b+'\target-io-completion-tests'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location ($b+'\src')
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features --lib user::clock::tests::native_thread_clock_excludes_another_threads_work -- --exact --nocapture > '+$b+'\native-wait-packet-associate-clock-isolated.log 2>&1')
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-wait-packet-associate-clock-isolated.log') -Destination 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\native-wait-packet-associate-clock-isolated.log'
exit $gateExit
