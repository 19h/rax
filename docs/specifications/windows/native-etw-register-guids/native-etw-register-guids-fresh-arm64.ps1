$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\etw-register-guids-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-etw-register-guids-fresh.cpp') -Destination ($b+'\native-etw-register-guids-fresh.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-etw-register-guids-fresh.cpp')).Hash -ne '824C32A32EE2EF57874F2D3B67C81F20BBE1740D95535FF47B622912BD79CBB5'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-etw-register-guids-fresh.cpp /link /OUT:native-etw-register-guids-fresh-arm64.exe > native-etw-register-guids-fresh-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-etw-register-guids-fresh-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-etw-register-guids-fresh-arm64.exe > native-etw-register-guids-fresh-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-etw-register-guids-fresh-arm64.log','native-etw-register-guids-fresh-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
