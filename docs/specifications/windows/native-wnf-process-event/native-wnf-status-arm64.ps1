$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\wnf-status-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-wnf-status.cpp') -Destination ($b+'\native-wnf-status.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-wnf-status.cpp')).Hash -ne '6B911B58E2314B2328187CA94C6866BA7F3CFD3E980C2928BAB9ADCCF15ECB3D'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-wnf-status.cpp /link /OUT:native-wnf-status-arm64.exe > native-wnf-status-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-wnf-status-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-wnf-status-arm64.exe > native-wnf-status-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-wnf-status-arm64.log','native-wnf-status-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
