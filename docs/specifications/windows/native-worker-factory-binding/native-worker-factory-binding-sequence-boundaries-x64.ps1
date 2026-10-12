$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\worker-factory-binding-sequence-boundaries-x64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-worker-factory-binding-sequence-boundaries.cpp') -Destination ($b+'\native-worker-factory-binding-sequence-boundaries.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-worker-factory-binding-sequence-boundaries.cpp')).Hash -ne '1FCDBF7B2F65AA91603EF8DA9AE31C941D90F94AEA0BA304884DEEA2CCB8B386'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch amd64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-worker-factory-binding-sequence-boundaries.cpp /link /OUT:native-worker-factory-binding-sequence-boundaries-x64.exe > native-worker-factory-binding-sequence-boundaries-x64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-worker-factory-binding-sequence-boundaries-x64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-worker-factory-binding-sequence-boundaries-x64.exe > native-worker-factory-binding-sequence-boundaries-x64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-worker-factory-binding-sequence-boundaries-x64.log','native-worker-factory-binding-sequence-boundaries-x64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
