$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\worker-factory-basic-probe-x64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-worker-factory-basic-probe.cpp') -Destination ($b+'\native-worker-factory-basic-probe.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-worker-factory-basic-probe.cpp')).Hash -ne 'F84C99A8C4F9774CC67520B78DB5EA46973BA1A431645810BA8131CBBEA55D44'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-worker-factory-basic-probe.cpp /Fe:native-worker-factory-basic-probe-x64.exe > native-worker-factory-basic-probe-x64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-worker-factory-basic-probe-x64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-worker-factory-basic-probe-x64.exe > native-worker-factory-basic-probe-x64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-worker-factory-basic-probe-x64.log','native-worker-factory-basic-probe-x64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
