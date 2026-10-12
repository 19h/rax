$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\wait-packet-association-lifetime-v2-x86-laa'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-wait-packet-association-lifetime-v2.cpp') -Destination ($b+'\native-wait-packet-association-lifetime-v2.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-wait-packet-association-lifetime-v2.cpp')).Hash -ne 'EDD36EA094117C6D9E4CD8351F0AF141F43EE378A335433CA7138433086B3BE0'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x86 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-wait-packet-association-lifetime-v2.cpp /link /LARGEADDRESSAWARE /OUT:native-wait-packet-association-lifetime-v2-x86-laa.exe > native-wait-packet-association-lifetime-v2-x86-laa-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-wait-packet-association-lifetime-v2-x86-laa-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-wait-packet-association-lifetime-v2-x86-laa.exe > native-wait-packet-association-lifetime-v2-x86-laa.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-wait-packet-association-lifetime-v2-x86-laa.log','native-wait-packet-association-lifetime-v2-x86-laa-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
