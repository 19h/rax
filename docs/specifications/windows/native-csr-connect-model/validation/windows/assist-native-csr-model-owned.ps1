$ErrorActionPreference='Stop'
$env:PATH='C:\Windows\Temp\assist-native-rax-20261010\toolchain\bin;'+$env:PATH; $env:RUSTC='C:\Windows\Temp\assist-native-rax-20261010\toolchain\bin\rustc.exe'; $env:CARGO_HOME='C:\Windows\Temp\assist-native-rax-20261010\cargo-home'; $env:CARGO_TARGET_DIR='C:\Windows\Temp\assist-native-rax-20261010\target-io-completion-owned'; & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64; 
$env:PATH='C:\Windows\Temp\assist-native-rax-20261010\llvm\bin;'+$env:PATH
$env:ASSIST_RESOURCE_PYTHON='C:\Windows\Temp\assist-native-rax-20261010\python\python.exe'
$env:RUSTFLAGS='-C target-feature=+crt-static';$env:CARGO_BUILD_JOBS='2'

Copy-Item -LiteralPath 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\shared-section-owned-cpp-build.py' -Destination 'C:\Windows\Temp\assist-native-rax-20261010\shared-section-owned-cpp-build.py'
& 'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe' -B 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\verify-csr-owning-sources.py' | Tee-Object -FilePath 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\native-csr-model-owning-source-verification.json'
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& 'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe' -B 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\verify-current-cpp-consumers.py'
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
Set-Location 'C:\Windows\Temp\assist-native-rax-20261010\assist\rust\assist-rs'
& cmd.exe /d /c 'cargo.exe build --locked --offline --features rax > C:\Windows\Temp\assist-native-rax-20261010\assist-native-csr-model-shipping.log 2>&1'
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath 'C:\Windows\Temp\assist-native-rax-20261010\assist-native-csr-model-shipping.log' -Destination 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\assist-native-csr-model-shipping.log'
Write-Output ('assist-native-csr-model-shipping.log '+(Get-FileHash -Algorithm SHA256 'C:\Windows\Temp\assist-native-rax-20261010\assist-native-csr-model-shipping.log').Hash)
if($gateExit -ne 0){exit $gateExit}
& cmd.exe /d /c 'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe C:\Windows\Temp\assist-native-rax-20261010\shared-section-owned-cpp-build.py > C:\Windows\Temp\assist-native-rax-20261010\assist-native-csr-model-cpp.log 2>&1'
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath 'C:\Windows\Temp\assist-native-rax-20261010\assist-native-csr-model-cpp.log' -Destination 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\assist-native-csr-model-cpp.log'
Write-Output ('assist-native-csr-model-cpp.log '+(Get-FileHash -Algorithm SHA256 'C:\Windows\Temp\assist-native-rax-20261010\assist-native-csr-model-cpp.log').Hash)
if($gateExit -ne 0){exit $gateExit}
Set-Location 'C:\Windows\Temp\assist-native-rax-20261010\assist\rust\assist-rs'
& cmd.exe /d /c 'cargo.exe build --locked --offline --features rax --message-format=json > C:\Windows\Temp\assist-native-rax-20261010\native-csr-model-after-artifacts.jsonl 2> C:\Windows\Temp\assist-native-rax-20261010\native-csr-model-after-artifacts-stderr.log'
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
$rlib=$null
foreach($line in (Get-Content 'C:\Windows\Temp\assist-native-rax-20261010\native-csr-model-after-artifacts.jsonl')){
 $j=$line|ConvertFrom-Json
 if($j.reason -eq 'compiler-artifact' -and $j.target.name -eq 'rax' -and $j.manifest_path -notlike '*\capi\Cargo.toml'){
  foreach($f in $j.filenames){if($f.EndsWith('.rlib')){$rlib=$f}}
 }
}
if(!$rlib){throw 'no current RAX rlib artifact'}
Write-Output ('validated rlib: '+$rlib)
& cmd.exe /d /c 'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\inspect-csr-model-archives.py > C:\Windows\Temp\assist-native-rax-20261010\native-csr-model-owning-archive-hashes.json'
$archiveExit=$LASTEXITCODE
Copy-Item -LiteralPath 'C:\Windows\Temp\assist-native-rax-20261010\native-csr-model-owning-archive-hashes.json' -Destination 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\native-csr-model-owning-archive-hashes.json'
exit $archiveExit
