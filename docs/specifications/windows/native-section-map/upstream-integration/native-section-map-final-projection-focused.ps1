$ErrorActionPreference='Stop'
Copy-Item -LiteralPath 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\latest-section-map-leaf-tests.rs' -Destination 'C:\Windows\Temp\assist-native-rax-20261010\src\src\user\windows\process\sched\tests\services_tests\section_map_leaf_tests.rs'
$env:PATH='C:\Windows\Temp\assist-native-rax-20261010\toolchain\bin;'+$env:PATH
$env:RUSTC='C:\Windows\Temp\assist-native-rax-20261010\toolchain\bin\rustc.exe'
$env:CARGO_HOME='C:\Windows\Temp\assist-native-rax-20261010\cargo-home'
$env:CARGO_TARGET_DIR='C:\Windows\Temp\assist-native-rax-20261010\target-section-map'
$env:RUSTFLAGS='-C target-feature=+crt-static'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location 'C:\Windows\Temp\assist-native-rax-20261010\src'
& cmd.exe /d /c 'cargo.exe test --locked --offline --no-default-features --lib native_section_map -- --nocapture > C:\Windows\Temp\assist-native-rax-20261010\native-section-map-final-projection-focused.log 2>&1'
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath 'C:\Windows\Temp\assist-native-rax-20261010\native-section-map-final-projection-focused.log' -Destination 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\native-section-map-final-projection-focused.log'
exit $gateExit
