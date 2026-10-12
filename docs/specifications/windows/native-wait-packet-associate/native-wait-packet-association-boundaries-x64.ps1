$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\wait-packet-association-boundaries-x64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-wait-packet-association-boundaries.cpp') -Destination ($b+'\native-wait-packet-association-boundaries.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-wait-packet-association-boundaries.cpp')).Hash -ne 'B0A8CB6FA5957D7762FEB74C9A1D7C78E6326BFFFCB6633F96D73229EEBB59AF'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch amd64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-wait-packet-association-boundaries.cpp /link /OUT:native-wait-packet-association-boundaries-x64.exe > native-wait-packet-association-boundaries-x64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-wait-packet-association-boundaries-x64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-wait-packet-association-boundaries-x64.exe > native-wait-packet-association-boundaries-x64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-wait-packet-association-boundaries-x64.log','native-wait-packet-association-boundaries-x64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
