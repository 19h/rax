$ErrorActionPreference='Stop'
$env:PATH='C:\Windows\Temp\assist-native-rax-20261010\toolchain\bin;'+$env:PATH; $env:RUSTC='C:\Windows\Temp\assist-native-rax-20261010\toolchain\bin\rustc.exe'; $env:CARGO_HOME='C:\Windows\Temp\assist-native-rax-20261010\cargo-home'; $env:CARGO_TARGET_DIR='C:\Windows\Temp\assist-native-rax-20261010\target-io-completion-owned'; & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64; 
$env:PATH='C:\Windows\Temp\assist-native-rax-20261010\llvm\bin;'+$env:PATH
$env:ASSIST_RESOURCE_PYTHON='C:\Windows\Temp\assist-native-rax-20261010\python\python.exe'
$env:RUSTFLAGS='-C target-feature=+crt-static'

Copy-Item -LiteralPath 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\scheduler-shared-owned-cpp-build.py' -Destination 'C:\Windows\Temp\assist-native-rax-20261010\scheduler-shared-owned-cpp-build.py'
& 'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe' -B 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\verify-scheduler-shared-owning-sources.py' | Tee-Object -FilePath 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\native-scheduler-shared-final-owning-source-verification.json'
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
Set-Location 'C:\Windows\Temp\assist-native-rax-20261010\assist\rust\assist-rs'
& cmd.exe /d /c 'cargo.exe build --locked --offline --features rax > C:\Windows\Temp\assist-native-rax-20261010\assist-native-scheduler-shared-owned-final-shipping.log 2>&1'
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath 'C:\Windows\Temp\assist-native-rax-20261010\assist-native-scheduler-shared-owned-final-shipping.log' -Destination 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\assist-native-scheduler-shared-owned-final-shipping.log'
Write-Output ('assist-native-scheduler-shared-owned-final-shipping.log '+(Get-FileHash -Algorithm SHA256 'C:\Windows\Temp\assist-native-rax-20261010\assist-native-scheduler-shared-owned-final-shipping.log').Hash)
if($gateExit -ne 0){exit $gateExit}
& cmd.exe /d /c 'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe C:\Windows\Temp\assist-native-rax-20261010\scheduler-shared-owned-cpp-build.py > C:\Windows\Temp\assist-native-rax-20261010\assist-native-scheduler-shared-owned-final-cpp.log 2>&1'
$gateExit=$LASTEXITCODE
Copy-Item -LiteralPath 'C:\Windows\Temp\assist-native-rax-20261010\assist-native-scheduler-shared-owned-final-cpp.log' -Destination 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\assist-native-scheduler-shared-owned-final-cpp.log'
Write-Output ('assist-native-scheduler-shared-owned-final-cpp.log '+(Get-FileHash -Algorithm SHA256 'C:\Windows\Temp\assist-native-rax-20261010\assist-native-scheduler-shared-owned-final-cpp.log').Hash)
if($gateExit -ne 0){exit $gateExit}
Set-Location 'C:\Windows\Temp\assist-native-rax-20261010\assist\rust\assist-rs'
& cmd.exe /d /c 'cargo.exe build --locked --offline --features rax --message-format=json > C:\Windows\Temp\assist-native-rax-20261010\native-scheduler-shared-final-after-artifacts.jsonl 2> C:\Windows\Temp\assist-native-rax-20261010\native-scheduler-shared-final-after-artifacts-stderr.log'
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
$rlib=$null
foreach($line in (Get-Content 'C:\Windows\Temp\assist-native-rax-20261010\native-scheduler-shared-final-after-artifacts.jsonl')){
 $j=$line|ConvertFrom-Json
 if($j.reason -eq 'compiler-artifact' -and $j.target.name -eq 'rax' -and $j.manifest_path -notlike '*\capi\Cargo.toml'){
  foreach($f in $j.filenames){if($f.EndsWith('.rlib')){$rlib=$f}}
 }
}
if(!$rlib){throw 'no current RAX rlib artifact'}
Write-Output ('validated rlib: '+$rlib)
Set-Location 'C:\Windows\Temp\assist-native-rax-20261010\assist'
& cl.exe /nologo /std:c++20 /MT /EHsc /utf-8 /DNOMINMAX /Iinclude /Ivendor/rax/capi/include /Ijson native-process-probe.cpp src/emulation/rax/rax_process.cpp ../target-io-completion-owned/debug/assist_rs.lib ws2_32.lib userenv.lib ntdll.lib bcrypt.lib advapi32.lib /Fe:native-cpp/native-process-probe.exe
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& cmd.exe /d /c 'native-cpp\native-process-probe.exe > C:\Windows\Temp\assist-native-rax-20261010\native-scheduler-shared-final-after-ordinary-processes.log 2>&1'
$probeExit=$LASTEXITCODE
if($probeExit -ne 0){exit $probeExit}
Copy-Item -LiteralPath 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\native-scheduler-shared-after-trace.rs' -Destination 'C:\Windows\Temp\assist-native-rax-20261010\native-scheduler-shared-after-trace.rs'
& cmd.exe /d /c ('rustc.exe --edition=2024 -C target-feature=+crt-static --crate-name native_production_diagnostic C:\Windows\Temp\assist-native-rax-20261010\native-scheduler-shared-after-trace.rs --extern "rax='+$rlib+'" -L dependency=C:\Windows\Temp\assist-native-rax-20261010\target-io-completion-owned\debug\deps -o C:\Windows\Temp\assist-native-rax-20261010\native-scheduler-shared-final-after-production-trace.exe > C:\Windows\Temp\assist-native-rax-20261010\native-scheduler-shared-final-after-production-trace-build.log 2>&1')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& cmd.exe /d /c 'C:\Windows\Temp\assist-native-rax-20261010\native-scheduler-shared-final-after-production-trace.exe C:\Windows\Temp\assist-native-rax-20261010\src\tests\fixtures\user\windows\bin\arm64\smoke.exe > C:\Windows\Temp\assist-native-rax-20261010\native-scheduler-shared-final-after-production-trace-output.log 2>&1'
$traceExit=$LASTEXITCODE
foreach($name in @('native-scheduler-shared-final-after-ordinary-processes.log','native-scheduler-shared-final-after-production-trace-output.log','native-scheduler-shared-final-after-production-trace-build.log','native-scheduler-shared-final-after-artifacts.jsonl','native-scheduler-shared-final-after-artifacts-stderr.log')){Copy-Item -LiteralPath ('C:\Windows\Temp\assist-native-rax-20261010\'+$name) -Destination ('C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ('C:\Windows\Temp\assist-native-rax-20261010\'+$name)).Hash)}

if($traceExit -ne 0){exit $traceExit}
& cmd.exe /d /c 'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\inspect-scheduler-shared-owning-archives.py > C:\Windows\Temp\assist-native-rax-20261010\native-scheduler-shared-final-owning-archive-hashes.json'
$archiveExit=$LASTEXITCODE
Copy-Item -LiteralPath 'C:\Windows\Temp\assist-native-rax-20261010\native-scheduler-shared-final-owning-archive-hashes.json' -Destination 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\native-scheduler-shared-final-owning-archive-hashes.json'
exit $archiveExit
