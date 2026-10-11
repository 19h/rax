$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\io-completion-probe-x86-laa'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-io-completion-create-probe.cpp') -Destination ($b+'\native-io-completion-create-probe.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-io-completion-create-probe.cpp')).Hash -ne 'F0556A19CF88CBF27FD3B91CAA27C10D6BD053CB90ED5A1A498A9AC5ED899AB9'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x86 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-io-completion-create-probe.cpp /Fe:native-io-completion-create-probe-x86-laa.exe /link /LARGEADDRESSAWARE > native-io-completion-create-probe-x86-laa-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-io-completion-create-probe-x86-laa-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-io-completion-create-probe-x86-laa.exe C:\Windows\Temp\assist-native-rax-20261010\src\tests\fixtures\user\windows\bin\arm64\smoke.exe > native-io-completion-create-probe-x86-laa.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-io-completion-create-probe-x86-laa.log','native-io-completion-create-probe-x86-laa-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
