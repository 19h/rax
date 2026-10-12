$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\etw-notification-event-x86'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-etw-notification-output-readonly.cpp') -Destination ($b+'\native-etw-notification-output-readonly.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-etw-notification-output-readonly.cpp')).Hash -ne '0DF1B8F8C32D0B15B32B459C0345B8A86A5271BB14BE78A0AB6C6314C8B1B9D0'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x86 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-etw-notification-output-readonly.cpp /link /OUT:native-etw-notification-output-readonly-x86.exe > native-etw-notification-output-readonly-x86-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-etw-notification-output-readonly-x86-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-etw-notification-output-readonly-x86.exe > native-etw-notification-output-readonly-x86.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-etw-notification-output-readonly-x86.log','native-etw-notification-output-readonly-x86-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
