$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\worker-factory-binding-x86'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-worker-factory-binding.cpp') -Destination ($b+'\native-worker-factory-binding.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-worker-factory-binding.cpp')).Hash -ne '6D39B9D619D35231FE2513446CEBF5E9E32ED76034AA7F1E75C2AFF7CC9DC22A'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x86 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-worker-factory-binding.cpp /link /OUT:native-worker-factory-binding-x86.exe > native-worker-factory-binding-x86-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-worker-factory-binding-x86-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-worker-factory-binding-x86.exe > native-worker-factory-binding-x86.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-worker-factory-binding-x86.log','native-worker-factory-binding-x86-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
