$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\wnf-subscribe-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-wnf-subscribe-delivery.cpp') -Destination ($b+'\native-wnf-subscribe-delivery.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-wnf-subscribe-delivery.cpp')).Hash -ne 'AE35F70B71EA8D7C5228DB27B7C523CAA24A639EC1A3B63D7C6FE67C7E0518CC'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-wnf-subscribe-delivery.cpp /link /OUT:native-wnf-subscribe-delivery-arm64.exe > native-wnf-subscribe-delivery-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-wnf-subscribe-delivery-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-wnf-subscribe-delivery-arm64.exe > native-wnf-subscribe-delivery-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-wnf-subscribe-delivery-arm64.log','native-wnf-subscribe-delivery-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
