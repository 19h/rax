$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
& ($b+'\python\python.exe') -B ($s+'\repair-session-id-epoch2.py')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
$env:RUSTC=$b+'\toolchain\bin\rustc.exe'
$env:CARGO_HOME=$b+'\cargo-home'
$env:CARGO_TARGET_DIR=$b+'\target-system-time-final'
$env:CARGO_BUILD_JOBS='2'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location ($b+'\src-token-session-id')
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features --lib token -- --nocapture --test-threads=1 > '+$b+'\native-session-id-epoch2-complete-focus.log 2>&1')
$code=$LASTEXITCODE
Copy-Item ($b+'\native-session-id-epoch2-complete-focus.log') ($s+'\native-session-id-epoch2-complete-focus.log')
@{gate='focus';'exit-code'=$code;log='native-session-id-epoch2-complete-focus.log'} | ConvertTo-Json | Set-Content -Encoding utf8 ($s+'\native-session-id-epoch2-complete-gates.json')
exit $code
