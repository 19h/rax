$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
$rlib=$null
foreach($line in Get-Content ($b+'\native-io-completion-after-artifacts.jsonl')) {
 try {$j=$line|ConvertFrom-Json} catch {continue}
 if($j.reason -eq 'compiler-artifact' -and $j.target.name -eq 'rax' -and $j.target.kind -contains 'lib') {
  foreach($f in $j.filenames){if($f.EndsWith('.rlib')){$rlib=$f}}
 }
}
if(!$rlib){throw 'no validated owning rlib'}
Copy-Item -LiteralPath ($share+'\native-worker-factory-before-trace.rs') -Destination ($b+'\native-worker-factory-before-trace.rs')
& cmd.exe /d /c ('rustc.exe --edition=2024 -C target-feature=+crt-static --crate-name native_worker_before '+$b+'\native-worker-factory-before-trace.rs --extern "rax='+$rlib+'" -L dependency='+$b+'\target-io-completion-owned\debug\deps -o '+$b+'\native-worker-factory-before-trace.exe > '+$b+'\native-worker-factory-before-trace-build.log 2>&1')
if($LASTEXITCODE -ne 0){Get-Content ($b+'\native-worker-factory-before-trace-build.log');exit $LASTEXITCODE}
& cmd.exe /d /c ($b+'\native-worker-factory-before-trace.exe '+$b+'\src\tests\fixtures\user\windows\bin\arm64\smoke.exe > '+$b+'\native-worker-factory-before-trace-output.log 2>&1')
$code=$LASTEXITCODE
foreach($name in @('native-worker-factory-before-trace-output.log','native-worker-factory-before-trace-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name)}
exit $code
