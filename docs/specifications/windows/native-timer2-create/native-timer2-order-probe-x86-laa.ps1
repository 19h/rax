$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\timer2-order-probe-x86-laa'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-timer2-order-probe.cpp') -Destination ($b+'\native-timer2-order-probe.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-timer2-order-probe.cpp')).Hash -ne 'B6FC79EF2F32FFA7619B10D435CED32C986FC1CC5383A5BDA93FD737AA2B34A2'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x86 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-timer2-order-probe.cpp /link /LARGEADDRESSAWARE /OUT:native-timer2-order-probe-x86-laa.exe > native-timer2-order-probe-x86-laa-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-timer2-order-probe-x86-laa-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-timer2-order-probe-x86-laa.exe > native-timer2-order-probe-x86-laa.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-timer2-order-probe-x86-laa.log','native-timer2-order-probe-x86-laa-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
