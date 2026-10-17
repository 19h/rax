$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$d=$b+'\system-wnf-subscribe-deny-v3'
[IO.Directory]::CreateDirectory($d)|Out-Null
Copy-Item ($s+'\native-system-wnf-subscribe-deny-v3.cpp') ($d+'\native-system-wnf-subscribe-deny-v3.cpp')
if((Get-FileHash ($d+'\native-system-wnf-subscribe-deny-v3.cpp')).Hash.ToLowerInvariant() -ne 'db2a699d042704b136e4ee87e0cb3ddbc5d5653d25d33ae339e74a06791d41ce'){throw 'fresh source drift'}
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location $d
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT /GS /we4473 native-system-wnf-subscribe-deny-v3.cpp /link advapi32.lib /SUBSYSTEM:CONSOLE,10.0 /OUT:native-system-wnf-subscribe-deny-v3.exe > native-system-wnf-subscribe-deny-v3-build.log 2>&1'
$code=$LASTEXITCODE
Copy-Item ($d+'\native-system-wnf-subscribe-deny-v3-build.log') ($s+'\native-system-wnf-subscribe-deny-v3-build.log')
if($code -ne 0){exit $code}
& ($b+'\python\python.exe') -B ($s+'\inspect-system-wnf-subscribe-pe.py') ($d+'\native-system-wnf-subscribe-deny-v3.exe') arm64 > ($s+'\native-system-wnf-subscribe-deny-v3-identity.json')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& cmd.exe /d /c 'native-system-wnf-subscribe-deny-v3.exe arm64-deny > native-system-wnf-subscribe-deny-v3.log 2>&1'
$code=$LASTEXITCODE
Copy-Item ($d+'\native-system-wnf-subscribe-deny-v3.log') ($s+'\native-system-wnf-subscribe-deny-v3.log')
exit $code
