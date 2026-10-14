$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\section-map-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-section-map-fresh.cpp') -Destination ($b+'\native-section-map-fresh.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-section-map-fresh.cpp')).Hash -ne '75D3C1459068B78ADF022A5F307FC195F1F2AE3649D8D8B2625625DA2C7F7A84'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT /we4473 native-section-map-fresh.cpp /link /OUT:native-section-map-fresh-arm64.exe > native-section-map-fresh-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-section-map-fresh-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-section-map-fresh-arm64.exe > native-section-map-fresh-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-section-map-fresh-arm64.log','native-section-map-fresh-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
@{'exit-code'=$gateExit;'source-hash'='75D3C1459068B78ADF022A5F307FC195F1F2AE3649D8D8B2625625DA2C7F7A84';'source-bytes'=(Get-Item ($b+'\native-section-map-fresh.cpp')).Length;'exe-sha256'=(Get-FileHash ($b+'\native-section-map-fresh-arm64.exe')).Hash;'exe-bytes'=(Get-Item ($b+'\native-section-map-fresh-arm64.exe')).Length}|ConvertTo-Json|Set-Content ($share+'\native-section-map-fresh-arm64-outcome.json')
exit $gateExit
