$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\thread-token-open-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-thread-token-open-fresh.cpp') -Destination ($b+'\native-thread-token-open-fresh.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-thread-token-open-fresh.cpp')).Hash -ne '0D7DBF9AD1F49F44B4BAF0DB5CEE76CFF8B39F70207319DDF92D91A77CC5AC71'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-thread-token-open-fresh.cpp /link /OUT:native-thread-token-open-fresh-arm64.exe > native-thread-token-open-fresh-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-thread-token-open-fresh-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-thread-token-open-fresh-arm64.exe > native-thread-token-open-fresh-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-thread-token-open-fresh-arm64.log','native-thread-token-open-fresh-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
@{'exit-code'=$gateExit;'source-hash'='0D7DBF9AD1F49F44B4BAF0DB5CEE76CFF8B39F70207319DDF92D91A77CC5AC71';'source-bytes'=(Get-Item ($b+'\native-thread-token-open-fresh.cpp')).Length;'exe-sha256'=(Get-FileHash ($b+'\native-thread-token-open-fresh-arm64.exe')).Hash;'exe-bytes'=(Get-Item ($b+'\native-thread-token-open-fresh-arm64.exe')).Length}|ConvertTo-Json|Set-Content ($share+'\native-thread-token-open-fresh-arm64-outcome.json')
exit $gateExit
