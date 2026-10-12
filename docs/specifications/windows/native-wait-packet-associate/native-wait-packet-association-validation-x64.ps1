$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\wait-packet-association-validation-x64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-wait-packet-association-validation.cpp') -Destination ($b+'\native-wait-packet-association-validation.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-wait-packet-association-validation.cpp')).Hash -ne 'A7B3AE9A1629BC78147FBB9CE55D2814330F4265F43F92994CB8E778D48FEC17'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch amd64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-wait-packet-association-validation.cpp /link /OUT:native-wait-packet-association-validation-x64.exe > native-wait-packet-association-validation-x64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-wait-packet-association-validation-x64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-wait-packet-association-validation-x64.exe > native-wait-packet-association-validation-x64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-wait-packet-association-validation-x64.log','native-wait-packet-association-validation-x64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
