$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$d=$b+'\system-wnf-subscribe-transitions'
[IO.Directory]::CreateDirectory($d)|Out-Null
Copy-Item ($s+'\native-system-wnf-subscribe-transitions.cpp') ($d+'\native-system-wnf-subscribe-transitions.cpp')
if((Get-FileHash ($d+'\native-system-wnf-subscribe-transitions.cpp')).Hash.ToLowerInvariant() -ne 'f624c61f9d6085ac6aaeb5f379c326fca826324efa59c0bcfd7b4b46fcf6f2d5'){throw 'fresh source drift'}
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location $d
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT /GS /we4473 native-system-wnf-subscribe-transitions.cpp /link /SUBSYSTEM:CONSOLE,10.0 /OUT:native-system-wnf-subscribe-transitions.exe > native-system-wnf-subscribe-transitions-build.log 2>&1'
$code=$LASTEXITCODE
Copy-Item ($d+'\native-system-wnf-subscribe-transitions-build.log') ($s+'\native-system-wnf-subscribe-transitions-build.log')
if($code -ne 0){exit $code}
& ($b+'\python\python.exe') -B ($s+'\inspect-system-wnf-subscribe-pe.py') ($d+'\native-system-wnf-subscribe-transitions.exe') arm64 > ($s+'\native-system-wnf-subscribe-transitions-identity.json')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& cmd.exe /d /c 'native-system-wnf-subscribe-transitions.exe > native-system-wnf-subscribe-transitions.log 2>&1'
$code=$LASTEXITCODE
Copy-Item ($d+'\native-system-wnf-subscribe-transitions.log') ($s+'\native-system-wnf-subscribe-transitions.log')
exit $code
