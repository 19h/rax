$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\etw-notification-event-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-etw-notification-return-fresh.cpp') -Destination ($b+'\native-etw-notification-return-fresh.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-etw-notification-return-fresh.cpp')).Hash -ne '1672AC6D883DD71A2B4B69B5B52F4A88616F0EB96A5E50391A0192B2A0770153'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-etw-notification-return-fresh.cpp /link /OUT:native-etw-notification-return-fresh-arm64.exe > native-etw-notification-return-fresh-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-etw-notification-return-fresh-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-etw-notification-return-fresh-arm64.exe > native-etw-notification-return-fresh-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-etw-notification-return-fresh-arm64.log','native-etw-notification-return-fresh-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
