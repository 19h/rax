$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$d=$b+'\system-wnf-subscribe-deny'
[IO.Directory]::CreateDirectory($d)|Out-Null
Copy-Item ($s+'\native-system-wnf-subscribe-deny.cpp') ($d+'\native-system-wnf-subscribe-deny.cpp')
if((Get-FileHash ($d+'\native-system-wnf-subscribe-deny.cpp')).Hash.ToLowerInvariant() -ne '79872ceb25b11e20506ed4d4f586c3ec7a89903f090a2420869827c3fac90c13'){throw 'fresh source drift'}
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location $d
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT /GS /we4473 native-system-wnf-subscribe-deny.cpp /link advapi32.lib /SUBSYSTEM:CONSOLE,10.0 /OUT:native-system-wnf-subscribe-deny.exe arm64-deny > native-system-wnf-subscribe-deny-build.log 2>&1'
$code=$LASTEXITCODE
Copy-Item ($d+'\native-system-wnf-subscribe-deny-build.log') ($s+'\native-system-wnf-subscribe-deny-build.log')
if($code -ne 0){exit $code}
& ($b+'\python\python.exe') -B ($s+'\inspect-system-wnf-subscribe-pe.py') ($d+'\native-system-wnf-subscribe-deny.exe') arm64 > ($s+'\native-system-wnf-subscribe-deny-identity.json')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& cmd.exe /d /c 'native-system-wnf-subscribe-deny.exe arm64-deny > native-system-wnf-subscribe-deny.log 2>&1'
$code=$LASTEXITCODE
Copy-Item ($d+'\native-system-wnf-subscribe-deny.log') ($s+'\native-system-wnf-subscribe-deny.log')
exit $code
