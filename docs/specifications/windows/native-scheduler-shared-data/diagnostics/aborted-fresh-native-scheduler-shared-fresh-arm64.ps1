$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\scheduler-shared-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-scheduler-shared-fresh.cpp') -Destination ($b+'\native-scheduler-shared-fresh.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-scheduler-shared-fresh.cpp')).Hash -ne 'F2F464983697E56DCF4FB89FB55875107F7C94E48B0075DB5CC1CA8E47CF5933'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-scheduler-shared-fresh.cpp /link /OUT:native-scheduler-shared-fresh-arm64.exe > native-scheduler-shared-fresh-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-scheduler-shared-fresh-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-scheduler-shared-fresh-arm64.exe > native-scheduler-shared-fresh-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-scheduler-shared-fresh-arm64.log','native-scheduler-shared-fresh-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
@{'exit-code'=$gateExit;'source-hash'='F2F464983697E56DCF4FB89FB55875107F7C94E48B0075DB5CC1CA8E47CF5933';'source-bytes'=9983;'exe-sha256'=(Get-FileHash ($b+'\native-scheduler-shared-fresh-arm64.exe')).Hash;'exe-bytes'=(Get-Item ($b+'\native-scheduler-shared-fresh-arm64.exe')).Length}|ConvertTo-Json|Set-Content ($share+'\native-scheduler-shared-fresh-arm64-outcome.json')
exit $gateExit
