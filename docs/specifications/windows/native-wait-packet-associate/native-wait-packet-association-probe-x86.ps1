$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\wait-packet-association-probe-x86'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-wait-packet-association-probe.cpp') -Destination ($b+'\native-wait-packet-association-probe.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-wait-packet-association-probe.cpp')).Hash -ne 'D56496B17269CE832630C3F50E809D79CCBD4ECB64195C0A7D772A502FF659AB'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x86 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-wait-packet-association-probe.cpp /link /OUT:native-wait-packet-association-probe-x86.exe > native-wait-packet-association-probe-x86-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-wait-packet-association-probe-x86-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-wait-packet-association-probe-x86.exe > native-wait-packet-association-probe-x86.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-wait-packet-association-probe-x86.log','native-wait-packet-association-probe-x86-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
