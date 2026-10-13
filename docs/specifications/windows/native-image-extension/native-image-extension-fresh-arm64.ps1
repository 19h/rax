$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\image-extension-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-image-extension-fresh.cpp') -Destination ($b+'\native-image-extension-fresh.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-image-extension-fresh.cpp')).Hash -ne '23FC27E238840D6BB4DD656B0BBDAEBE77997A2DD76494D90245E9AC72F54023'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-image-extension-fresh.cpp /link /OUT:native-image-extension-fresh-arm64.exe > native-image-extension-fresh-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-image-extension-fresh-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-image-extension-fresh-arm64.exe > native-image-extension-fresh-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-image-extension-fresh-arm64.log','native-image-extension-fresh-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
@{'exit-code'=$gateExit;'source-hash'='23FC27E238840D6BB4DD656B0BBDAEBE77997A2DD76494D90245E9AC72F54023';'source-bytes'=9983;'exe-sha256'=(Get-FileHash ($b+'\native-image-extension-fresh-arm64.exe')).Hash;'exe-bytes'=(Get-Item ($b+'\native-image-extension-fresh-arm64.exe')).Length}|ConvertTo-Json|Set-Content ($share+'\native-image-extension-fresh-arm64-outcome.json')
exit $gateExit
