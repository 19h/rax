$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\etw-provider-traits-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-etw-provider-traits-fresh-payload.cpp') -Destination ($b+'\native-etw-provider-traits-fresh-payload.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-etw-provider-traits-fresh-payload.cpp')).Hash -ne '4C403AC95ED17CAEE4F94FB0B09A39086273E6E9C405BEB868EAFCD356C2A8D6'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-etw-provider-traits-fresh-payload.cpp /link /OUT:native-etw-provider-traits-fresh-payload-arm64.exe > native-etw-provider-traits-fresh-payload-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-etw-provider-traits-fresh-payload-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-etw-provider-traits-fresh-payload-arm64.exe > native-etw-provider-traits-fresh-payload-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-etw-provider-traits-fresh-payload-arm64.log','native-etw-provider-traits-fresh-payload-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
