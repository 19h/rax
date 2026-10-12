$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\wnf-process-event-fresh-x86'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-wnf-process-event-fresh.cpp') -Destination ($b+'\native-wnf-process-event-fresh.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-wnf-process-event-fresh.cpp')).Hash -ne '9CB8E150B470737D7481A0FCBDC52534367BFFCA8C33603AE9DCAEDCDBC9D12C'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x86 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-wnf-process-event-fresh.cpp /link /OUT:native-wnf-process-event-fresh-x86.exe > native-wnf-process-event-fresh-x86-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-wnf-process-event-fresh-x86-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-wnf-process-event-fresh-x86.exe > native-wnf-process-event-fresh-x86.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-wnf-process-event-fresh-x86.log','native-wnf-process-event-fresh-x86-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
