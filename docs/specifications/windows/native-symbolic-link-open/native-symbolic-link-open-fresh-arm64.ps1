$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\symbolic-link-open-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-symbolic-link-open-fresh.cpp') -Destination ($b+'\native-symbolic-link-open-fresh.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-symbolic-link-open-fresh.cpp')).Hash -ne '3C1511844A3F2A6CC26C1BA72AFA84E970448DEF8D8A7628CE5DA584B72996C2'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-symbolic-link-open-fresh.cpp /link /OUT:native-symbolic-link-open-fresh-arm64.exe > native-symbolic-link-open-fresh-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-symbolic-link-open-fresh-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-symbolic-link-open-fresh-arm64.exe > native-symbolic-link-open-fresh-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-symbolic-link-open-fresh-arm64.log','native-symbolic-link-open-fresh-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
