$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Copy-Item -LiteralPath ($s+'\native-wnf-subscribe-name-trace.rs') -Destination ($b+'\native-wnf-subscribe-name-trace.rs')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-wnf-subscribe-name-trace.rs')).Hash -ne '980BCB8B668C1BD7E0D3FD50BF8CAF81514CB9A9B465E5525558BE8702F97585'){throw 'source hash'}
$rlib=$null
foreach($line in (Get-Content ($b+'\native-wnf-process-event-after-artifacts.jsonl'))){$j=$line|ConvertFrom-Json;if($j.reason -eq 'compiler-artifact' -and $j.target.name -eq 'rax' -and $j.manifest_path -notlike '*\capi\Cargo.toml'){foreach($f in $j.filenames){if($f.EndsWith('.rlib')){$rlib=$f}}}}
if(!$rlib){throw 'no current core artifact'}
& cmd.exe /d /c ('rustc.exe --edition=2024 -C target-feature=+crt-static --crate-name native_wnf_name_diagnostic '+$b+'\native-wnf-subscribe-name-trace.rs --extern "rax='+$rlib+'" -L dependency='+$b+'\target-io-completion-owned\debug\deps -o '+$b+'\native-wnf-subscribe-name-trace.exe > '+$b+'\native-wnf-subscribe-name-trace-build.log 2>&1')
if($LASTEXITCODE -ne 0){Get-Content ($b+'\native-wnf-subscribe-name-trace-build.log');exit $LASTEXITCODE}
& cmd.exe /d /c ($b+'\native-wnf-subscribe-name-trace.exe '+$b+'\src\tests\fixtures\user\windows\bin\arm64\smoke.exe > '+$b+'\native-wnf-subscribe-name-trace.log 2>&1')
$gateExit=$LASTEXITCODE
foreach($n in @('native-wnf-subscribe-name-trace-build.log','native-wnf-subscribe-name-trace.log')){Copy-Item -LiteralPath ($b+'\'+$n) -Destination ($s+'\'+$n)}
exit $gateExit
