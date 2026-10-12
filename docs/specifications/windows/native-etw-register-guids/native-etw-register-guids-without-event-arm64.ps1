$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\etw-register-guids-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-etw-register-guids-without-event.cpp') -Destination ($b+'\native-etw-register-guids-without-event.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-etw-register-guids-without-event.cpp')).Hash -ne '8FD9C28A8E8268C25F0A4F9E92B0CCDA104E666801873B63D1D2B385F2429D48'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-etw-register-guids-without-event.cpp /link /OUT:native-etw-register-guids-without-event-arm64.exe > native-etw-register-guids-without-event-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-etw-register-guids-without-event-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-etw-register-guids-without-event-arm64.exe > native-etw-register-guids-without-event-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-etw-register-guids-without-event-arm64.log','native-etw-register-guids-without-event-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
