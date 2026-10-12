$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\object-directory-open-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-object-directory-open-fresh.cpp') -Destination ($b+'\native-object-directory-open-fresh.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-object-directory-open-fresh.cpp')).Hash -ne '6EA3E658113988360365754E0B2CEBA292FBF4EE1A66DECA0129071621FD698A'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-object-directory-open-fresh.cpp /link /OUT:native-object-directory-open-fresh-arm64.exe > native-object-directory-open-fresh-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-object-directory-open-fresh-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-object-directory-open-fresh-arm64.exe > native-object-directory-open-fresh-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-object-directory-open-fresh-arm64.log','native-object-directory-open-fresh-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
