$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\worker-factory-default-stack-probe-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-worker-factory-default-stack-probe.cpp') -Destination ($b+'\native-worker-factory-default-stack-probe.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-worker-factory-default-stack-probe.cpp')).Hash -ne 'AF5CD4670BA8BDB500C90EAC6E2B3A6C7D2F26A3375C5D7585A0000DCAA6CFAD'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-worker-factory-default-stack-probe.cpp /link /STACK:2097152,32768 /OUT:native-worker-factory-default-stack-probe-arm64.exe > native-worker-factory-default-stack-probe-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-worker-factory-default-stack-probe-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-worker-factory-default-stack-probe-arm64.exe > native-worker-factory-default-stack-probe-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-worker-factory-default-stack-probe-arm64.log','native-worker-factory-default-stack-probe-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
