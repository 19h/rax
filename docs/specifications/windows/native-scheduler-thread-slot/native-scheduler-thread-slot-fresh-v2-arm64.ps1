$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\scheduler-thread-slot-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-scheduler-thread-slot-fresh-v2.cpp') -Destination ($b+'\native-scheduler-thread-slot-fresh-v2.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-scheduler-thread-slot-fresh-v2.cpp')).Hash -ne '22E56C6C20789E40ADA7B8A3C5331CB8AD9F3EDD9B2E233789F918D5272A609B'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-scheduler-thread-slot-fresh-v2.cpp /link /OUT:native-scheduler-thread-slot-fresh-v2-arm64.exe > native-scheduler-thread-slot-fresh-v2-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-scheduler-thread-slot-fresh-v2-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-scheduler-thread-slot-fresh-v2-arm64.exe > native-scheduler-thread-slot-fresh-v2-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-scheduler-thread-slot-fresh-v2-arm64.log','native-scheduler-thread-slot-fresh-v2-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
@{'exit-code'=$gateExit;'source-hash'='22E56C6C20789E40ADA7B8A3C5331CB8AD9F3EDD9B2E233789F918D5272A609B';'source-bytes'=12320;'exe-sha256'=(Get-FileHash ($b+'\native-scheduler-thread-slot-fresh-v2-arm64.exe')).Hash;'exe-bytes'=(Get-Item ($b+'\native-scheduler-thread-slot-fresh-v2-arm64.exe')).Length}|ConvertTo-Json|Set-Content ($share+'\native-scheduler-thread-slot-fresh-v2-arm64-outcome.json')
exit $gateExit
