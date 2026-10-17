$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
$env:RUSTC=$b+'\toolchain\bin\rustc.exe'
$env:CARGO_HOME=$b+'\cargo-home'
$env:CARGO_TARGET_DIR=$b+'\target-system-time-final'
$env:CARGO_BUILD_JOBS='2'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location ($b+'\src-system-time')
$rlib=$null
foreach($line in (Get-Content ($b+'\native-system-time-final-build-user-artifacts.jsonl'))){
 $j=$line|ConvertFrom-Json
 if($j.reason -eq 'compiler-artifact' -and $j.target.name -eq 'rax' -and $j.manifest_path -notlike '*\capi\Cargo.toml'){
  foreach($f in $j.filenames){if($f.EndsWith('.rlib')){$rlib=$f}}
 }
}
if(!$rlib){throw 'no current core RAX rlib artifact'}
$observer=$s+'\dynamic-table-observer.rs'
if((Get-FileHash -Algorithm SHA256 -LiteralPath $observer).Hash.ToLowerInvariant() -ne '705af66f2eb297dadb57dd08ec2e7bbf638d5d3f3dc2c24e33c3d003858170fb'){throw 'observer source changed'}
& cmd.exe /d /c ('rustc.exe --edition=2024 --crate-name native_console_owner_trace '+$observer+' --extern rax='+$rlib+' -L dependency='+$env:CARGO_TARGET_DIR+'\debug\deps -o '+$b+'\dynamic-table-observer.exe > '+$b+'\dynamic-table-observer-build.log 2>&1')
$exitCode=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\dynamic-table-observer-build.log') -Destination ($s+'\dynamic-table-observer-build.log')
if($exitCode -ne 0){exit $exitCode}

& cmd.exe /d /c ($b+'\dynamic-table-observer.exe '+$b+'\src-system-time\tests\fixtures\user\windows\bin\arm64\smoke.exe > '+$s+'\dynamic-table-smoke-observer.log 2>&1')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& cmd.exe /d /c ($b+'\dynamic-table-observer.exe C:\Windows\System32\whoami.exe > '+$s+'\dynamic-table-whoami-observer.log 2>&1')
exit $LASTEXITCODE
