$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$d=$b+'\system-wnf-subscribe-triples'
[IO.Directory]::CreateDirectory($d)|Out-Null
Copy-Item ($s+'\native-system-wnf-subscribe-triples.cpp') ($d+'\native-system-wnf-subscribe-triples.cpp')
if((Get-FileHash ($d+'\native-system-wnf-subscribe-triples.cpp')).Hash.ToLowerInvariant() -ne 'fa50f8d27775a9df12f44b68f26a43e6f86f446f6063e71718827c00b785a396'){throw 'fresh source drift'}
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location $d
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT /GS /we4473 native-system-wnf-subscribe-triples.cpp /link /SUBSYSTEM:CONSOLE,10.0 /OUT:native-system-wnf-subscribe-triples.exe > native-system-wnf-subscribe-triples-build.log 2>&1'
$code=$LASTEXITCODE
Copy-Item ($d+'\native-system-wnf-subscribe-triples-build.log') ($s+'\native-system-wnf-subscribe-triples-build.log')
if($code -ne 0){exit $code}
& ($b+'\python\python.exe') -B ($s+'\inspect-system-wnf-subscribe-pe.py') ($d+'\native-system-wnf-subscribe-triples.exe') arm64 > ($s+'\native-system-wnf-subscribe-triples-identity.json')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& cmd.exe /d /c 'native-system-wnf-subscribe-triples.exe > native-system-wnf-subscribe-triples.log 2>&1'
$code=$LASTEXITCODE
Copy-Item ($d+'\native-system-wnf-subscribe-triples.log') ($s+'\native-system-wnf-subscribe-triples.log')
exit $code
