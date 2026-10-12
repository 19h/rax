$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\etw-notification-event-x64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-etw-notification-boundaries.cpp') -Destination ($b+'\native-etw-notification-boundaries.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-etw-notification-boundaries.cpp')).Hash -ne '704C129FBDDA1CB0FE0A39002452A13909EFF83617888D871FC1F9D17AF0A4E6'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch amd64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-etw-notification-boundaries.cpp /link /OUT:native-etw-notification-boundaries-x64.exe > native-etw-notification-boundaries-x64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-etw-notification-boundaries-x64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-etw-notification-boundaries-x64.exe > native-etw-notification-boundaries-x64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-etw-notification-boundaries-x64.log','native-etw-notification-boundaries-x64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
