$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010';$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
$env:RUSTC=$b+'\toolchain\bin\rustc.exe'
$env:CARGO_HOME=$b+'\cargo-home'
$env:CARGO_TARGET_DIR=$b+'\target-bno-isolation-prototype'
$env:CARGO_BUILD_JOBS='2';$env:CARGO_INCREMENTAL='0'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location ($b+'\src-object-reparse-epoch1')
& cmd.exe /d /c ('cargo.exe test --locked --offline --no-default-features --lib native_object_reparse -- --nocapture --test-threads=1 > '+$b+'\links-native-focus-epoch1.log 2>&1')
$code=$LASTEXITCODE
Copy-Item ($b+'\links-native-focus-epoch1.log') ($s+'\links-native-focus-epoch1.log')
exit $code
