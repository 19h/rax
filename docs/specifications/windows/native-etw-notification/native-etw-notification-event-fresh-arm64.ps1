$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\etw-notification-event-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-etw-notification-event-fresh.cpp') -Destination ($b+'\native-etw-notification-event-fresh.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-etw-notification-event-fresh.cpp')).Hash -ne 'BDA90369851759A633E1A57A919741549DC694A437C1140E7C9F291DF6F1AE5B'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-etw-notification-event-fresh.cpp /link /OUT:native-etw-notification-event-fresh-arm64.exe > native-etw-notification-event-fresh-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-etw-notification-event-fresh-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-etw-notification-event-fresh-arm64.exe > native-etw-notification-event-fresh-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-etw-notification-event-fresh-arm64.log','native-etw-notification-event-fresh-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
