$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Copy-Item -LiteralPath ($s+'\native-etw-notification-input-trace.rs') -Destination ($b+'\native-etw-notification-input-trace.rs')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-etw-notification-input-trace.rs')).Hash -ne '587F696BCB275D3937BDFF6E80B1B181E399E89965CBCA7379BF81C6B908A993'){throw 'source hash'}
$rlib=$null
foreach($line in (Get-Content ($b+'\native-wnf-subscribe-after-artifacts.jsonl'))){$j=$line|ConvertFrom-Json;if($j.reason -eq 'compiler-artifact' -and $j.target.name -eq 'rax' -and $j.manifest_path -notlike '*\capi\Cargo.toml'){foreach($f in $j.filenames){if($f.EndsWith('.rlib')){$rlib=$f}}}}
if(!$rlib){throw 'no current core artifact'}
& cmd.exe /d /c ('rustc.exe --edition=2024 -C target-feature=+crt-static --crate-name native_etw_input_diagnostic '+$b+'\native-etw-notification-input-trace.rs --extern "rax='+$rlib+'" -L dependency='+$b+'\target-io-completion-owned\debug\deps -o '+$b+'\native-etw-notification-input-trace.exe > '+$b+'\native-etw-notification-input-trace-build.log 2>&1')
if($LASTEXITCODE -ne 0){Get-Content ($b+'\native-etw-notification-input-trace-build.log');exit $LASTEXITCODE}
& cmd.exe /d /c ($b+'\native-etw-notification-input-trace.exe '+$b+'\src\tests\fixtures\user\windows\bin\arm64\smoke.exe > '+$b+'\native-etw-notification-input-trace.log 2>&1')
$gateExit=$LASTEXITCODE
foreach($n in @('native-etw-notification-input-trace-build.log','native-etw-notification-input-trace.log')){Copy-Item -LiteralPath ($b+'\'+$n) -Destination ($s+'\'+$n)}
exit $gateExit
