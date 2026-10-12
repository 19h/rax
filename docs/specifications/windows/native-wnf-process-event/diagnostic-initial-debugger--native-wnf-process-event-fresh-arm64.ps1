$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\wnf-process-event-fresh-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-wnf-process-event-fresh.cpp') -Destination ($b+'\native-wnf-process-event-fresh.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-wnf-process-event-fresh.cpp')).Hash -ne '73A377731B5310995AB1014962C0917832E80439F1957E6CB16E4C61B8C1A6E9'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-wnf-process-event-fresh.cpp /link /OUT:native-wnf-process-event-fresh-arm64.exe > native-wnf-process-event-fresh-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-wnf-process-event-fresh-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-wnf-process-event-fresh-arm64.exe > native-wnf-process-event-fresh-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-wnf-process-event-fresh-arm64.log','native-wnf-process-event-fresh-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
