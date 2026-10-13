$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010';$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Copy-Item -LiteralPath ($s+'\native-symbolic-link-open-guest-trace.rs') -Destination ($b+'\native-symbolic-link-open-guest-trace.rs')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-symbolic-link-open-guest-trace.rs')).Hash -ne '3AF4A83136A314D3D50713411E5EFB3B464163B7FA427798DFCF2454B0B9607A'){throw 'observer source mismatch'}
$rlib=$null
foreach($line in (Get-Content ($b+'\native-object-directory-open-after-artifacts.jsonl'))){$j=$line|ConvertFrom-Json;if($j.reason -eq 'compiler-artifact' -and $j.target.name -eq 'rax' -and $j.manifest_path -notlike '*\capi\Cargo.toml'){if($j.features.Count -ne 0){throw 'core features not empty'};foreach($f in $j.filenames){if($f.EndsWith('.rlib')){$rlib=$f}}}}
if(!$rlib){throw 'no verified core artifact'}
& cmd.exe /d /c ('rustc.exe --edition=2024 -C target-feature=+crt-static --crate-name native_directory_diagnostic '+$b+'\native-symbolic-link-open-guest-trace.rs --extern "rax='+$rlib+'" -L dependency='+$b+'\target-io-completion-owned\debug\deps -o '+$b+'\native-symbolic-link-open-guest-trace.exe > '+$b+'\native-symbolic-link-open-guest-trace-build.log 2>&1')
if($LASTEXITCODE -ne 0){Get-Content ($b+'\native-symbolic-link-open-guest-trace-build.log');exit $LASTEXITCODE}
& cmd.exe /d /c ($b+'\native-symbolic-link-open-guest-trace.exe '+$b+'\src\tests\fixtures\user\windows\bin\arm64\smoke.exe > '+$b+'\native-symbolic-link-open-guest-trace.log 2>&1')
$gateExit=$LASTEXITCODE
foreach($name in @('native-symbolic-link-open-guest-trace.log','native-symbolic-link-open-guest-trace-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($s+'\'+$name)}
@{'exit-code'=$gateExit}|ConvertTo-Json|Set-Content -Encoding utf8 -LiteralPath ($s+'\native-symbolic-link-open-guest-trace-exit.json')
exit $gateExit
