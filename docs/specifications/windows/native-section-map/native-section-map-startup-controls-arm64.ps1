$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\section-map-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-section-map-startup-controls.cpp') -Destination ($b+'\native-section-map-startup-controls.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-section-map-startup-controls.cpp')).Hash -ne '66C107BEC84D2F2EAEB9132C85889A08D9681E9B18360DC1CBA08C49D1AC257E'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT /we4473 native-section-map-startup-controls.cpp /link /OUT:native-section-map-startup-controls-arm64.exe > native-section-map-startup-controls-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-section-map-startup-controls-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-section-map-startup-controls-arm64.exe > native-section-map-startup-controls-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-section-map-startup-controls-arm64.log','native-section-map-startup-controls-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
@{'exit-code'=$gateExit;'source-hash'='66C107BEC84D2F2EAEB9132C85889A08D9681E9B18360DC1CBA08C49D1AC257E';'source-bytes'=(Get-Item ($b+'\native-section-map-startup-controls.cpp')).Length;'exe-sha256'=(Get-FileHash ($b+'\native-section-map-startup-controls-arm64.exe')).Hash;'exe-bytes'=(Get-Item ($b+'\native-section-map-startup-controls-arm64.exe')).Length}|ConvertTo-Json|Set-Content ($share+'\native-section-map-startup-controls-arm64-outcome.json')
exit $gateExit
