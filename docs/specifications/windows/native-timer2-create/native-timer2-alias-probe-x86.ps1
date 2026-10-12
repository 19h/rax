$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\timer2-alias-probe-x86'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-timer2-alias-probe.cpp') -Destination ($b+'\native-timer2-alias-probe.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-timer2-alias-probe.cpp')).Hash -ne '4B18F5377C6E1E0B100351855A6D283A0B5B2AD472166BAB9402519325EA1908'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x86 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-timer2-alias-probe.cpp /link /OUT:native-timer2-alias-probe-x86.exe > native-timer2-alias-probe-x86-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-timer2-alias-probe-x86-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-timer2-alias-probe-x86.exe > native-timer2-alias-probe-x86.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-timer2-alias-probe-x86.log','native-timer2-alias-probe-x86-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
