$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\section-open-arm64'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-section-open-fresh-v3.cpp') -Destination ($b+'\native-section-open-fresh-v3.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-section-open-fresh-v3.cpp')).Hash -ne '3F8335B3DFF66629BA7A7C88C0D1E32CB01FF36BFA00106933E5F9FE5988FAEB'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT /we4473 native-section-open-fresh-v3.cpp /link /OUT:native-section-open-fresh-v3-arm64.exe > native-section-open-fresh-v3-arm64-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-section-open-fresh-v3-arm64-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-section-open-fresh-v3-arm64.exe > native-section-open-fresh-v3-arm64.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-section-open-fresh-v3-arm64.log','native-section-open-fresh-v3-arm64-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
@{'exit-code'=$gateExit;'source-hash'='3F8335B3DFF66629BA7A7C88C0D1E32CB01FF36BFA00106933E5F9FE5988FAEB';'source-bytes'=(Get-Item ($b+'\native-section-open-fresh-v3.cpp')).Length;'exe-sha256'=(Get-FileHash ($b+'\native-section-open-fresh-v3-arm64.exe')).Hash;'exe-bytes'=(Get-Item ($b+'\native-section-open-fresh-v3-arm64.exe')).Length}|ConvertTo-Json|Set-Content ($share+'\native-section-open-fresh-v3-arm64-outcome.json')
exit $gateExit
