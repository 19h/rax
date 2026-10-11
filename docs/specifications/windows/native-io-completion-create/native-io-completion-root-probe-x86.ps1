$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\io-completion-root-probe-x86'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-io-completion-root-probe.cpp') -Destination ($b+'\native-io-completion-root-probe.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-io-completion-root-probe.cpp')).Hash -ne 'F1982F797DF8CF6398E79C72ACAC1C42012D530F1F4AC5E36CFC6AF40F3CD3FB'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x86 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-io-completion-root-probe.cpp /Fe:native-io-completion-root-probe-x86.exe > native-io-completion-root-probe-x86-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-io-completion-root-probe-x86-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-io-completion-root-probe-x86.exe C:\Windows\Temp\assist-native-rax-20261010\src\tests\fixtures\user\windows\bin\arm64\smoke.exe > native-io-completion-root-probe-x86.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-io-completion-root-probe-x86.log','native-io-completion-root-probe-x86-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
