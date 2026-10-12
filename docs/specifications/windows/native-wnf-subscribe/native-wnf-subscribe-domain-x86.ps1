$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\wnf-subscribe-x86'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-wnf-subscribe-domain.cpp') -Destination ($b+'\native-wnf-subscribe-domain.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-wnf-subscribe-domain.cpp')).Hash -ne 'C34C13802CFDEACAF1C1F18592DA4254822B559E94C01E0714A35A80811859B4'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x86 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-wnf-subscribe-domain.cpp /link /OUT:native-wnf-subscribe-domain-x86.exe > native-wnf-subscribe-domain-x86-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-wnf-subscribe-domain-x86-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-wnf-subscribe-domain-x86.exe > native-wnf-subscribe-domain-x86.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-wnf-subscribe-domain-x86.log','native-wnf-subscribe-domain-x86-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
