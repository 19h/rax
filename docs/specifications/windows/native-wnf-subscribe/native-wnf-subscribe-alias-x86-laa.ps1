$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\wnf-subscribe-x86-laa'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-wnf-subscribe-alias.cpp') -Destination ($b+'\native-wnf-subscribe-alias.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-wnf-subscribe-alias.cpp')).Hash -ne 'F0C5DAFAB13FC6937E690DA7CEA7D2089E7CD5817DD0734A5F4851860698DCB9'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x86 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-wnf-subscribe-alias.cpp /link /LARGEADDRESSAWARE /OUT:native-wnf-subscribe-alias-x86-laa.exe > native-wnf-subscribe-alias-x86-laa-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-wnf-subscribe-alias-x86-laa-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-wnf-subscribe-alias-x86-laa.exe > native-wnf-subscribe-alias-x86-laa.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-wnf-subscribe-alias-x86-laa.log','native-wnf-subscribe-alias-x86-laa-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
