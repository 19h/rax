$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\worker-factory-soft-maximum-x86-laa'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-worker-factory-soft-maximum.cpp') -Destination ($b+'\native-worker-factory-soft-maximum.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-worker-factory-soft-maximum.cpp')).Hash -ne '20BE8FD596D8C8F61A9AE010013650DAF61EDE3A2D5CD99EA459769EF87AC0A0'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x86 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-worker-factory-soft-maximum.cpp /link /LARGEADDRESSAWARE /OUT:native-worker-factory-soft-maximum-x86-laa.exe > native-worker-factory-soft-maximum-x86-laa-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-worker-factory-soft-maximum-x86-laa-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-worker-factory-soft-maximum-x86-laa.exe > native-worker-factory-soft-maximum-x86-laa.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-worker-factory-soft-maximum-x86-laa.log','native-worker-factory-soft-maximum-x86-laa-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
