$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010\wnf-subscribe-x86-laa'
[IO.Directory]::CreateDirectory($b)|Out-Null
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
Copy-Item -LiteralPath ($share+'\native-wnf-subscribe-guards.cpp') -Destination ($b+'\native-wnf-subscribe-guards.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-wnf-subscribe-guards.cpp')).Hash -ne '3A2002DC6721C1BE705DE706C82C48A8CCEBF4A916CD7FC3E45ACC4447FB4151'){throw 'source hash mismatch'}
Set-Location $b
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch x86 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT native-wnf-subscribe-guards.cpp /link /LARGEADDRESSAWARE /OUT:native-wnf-subscribe-guards-x86-laa.exe > native-wnf-subscribe-guards-x86-laa-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content 'native-wnf-subscribe-guards-x86-laa-build.log';exit $LASTEXITCODE}
& cmd.exe /d /c 'native-wnf-subscribe-guards-x86-laa.exe > native-wnf-subscribe-guards-x86-laa.log 2>&1'
$gateExit=$LASTEXITCODE
foreach($name in @('native-wnf-subscribe-guards-x86-laa.log','native-wnf-subscribe-guards-x86-laa-build.log')){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
exit $gateExit
