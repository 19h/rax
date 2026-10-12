$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\wait-packet-association-boundaries-x86-laa'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-wait-packet-association-boundaries.cpp') -Destination ($b+'\native-wait-packet-association-boundaries.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-wait-packet-association-boundaries.cpp')).Hash -ne 'B0A8CB6FA5957D7762FEB74C9A1D7C78E6326BFFFCB6633F96D73229EEBB59AF'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x86 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-wait-packet-association-boundaries.cpp /link /LARGEADDRESSAWARE /OUT:native-wait-packet-association-boundaries-x86-laa.exe > native-wait-packet-association-boundaries-x86-laa-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-wait-packet-association-boundaries-x86-laa-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-wait-packet-association-boundaries-x86-laa.exe > native-wait-packet-association-boundaries-x86-laa.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-wait-packet-association-boundaries-x86-laa.log','native-wait-packet-association-boundaries-x86-laa-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
