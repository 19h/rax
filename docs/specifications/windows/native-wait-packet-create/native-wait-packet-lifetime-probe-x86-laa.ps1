$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\wait-packet-lifetime-probe-x86-laa'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-wait-packet-lifetime-probe.cpp') -Destination ($b+'\native-wait-packet-lifetime-probe.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-wait-packet-lifetime-probe.cpp')).Hash -ne 'A07293C8BE4E87CB643FE68C7A8F222F2EEC0F39E464A9D07A041F0A38DFACB6'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x86 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-wait-packet-lifetime-probe.cpp /link /LARGEADDRESSAWARE /OUT:native-wait-packet-lifetime-probe-x86-laa.exe > native-wait-packet-lifetime-probe-x86-laa-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-wait-packet-lifetime-probe-x86-laa-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-wait-packet-lifetime-probe-x86-laa.exe > native-wait-packet-lifetime-probe-x86-laa.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-wait-packet-lifetime-probe-x86-laa.log','native-wait-packet-lifetime-probe-x86-laa-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
