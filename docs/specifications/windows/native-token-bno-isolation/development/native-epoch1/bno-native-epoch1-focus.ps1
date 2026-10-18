$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
$env:RUSTC=$b+'\toolchain\bin\rustc.exe'
$env:CARGO_HOME=$b+'\cargo-home'
$env:CARGO_TARGET_DIR=$b+'\target-bno-isolation-prototype'
$env:CARGO_BUILD_JOBS='1'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location ($b+'\src-bno-isolation-prototype')
$command='cargo.exe test --locked --offline --no-default-features --lib bno_ -- --nocapture --test-threads=1'
& cmd.exe /d /c ($command+' > '+$b+'\bno-native-epoch1-focus.log 2>&1')
$code=$LASTEXITCODE
Copy-Item ($b+'\bno-native-epoch1-focus.log') ($s+'\bno-native-epoch1-focus.log')
exit $code
