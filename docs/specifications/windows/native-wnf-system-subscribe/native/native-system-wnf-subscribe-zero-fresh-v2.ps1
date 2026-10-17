$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$d=$b+'\system-wnf-subscribe-zero-fresh-v2'
[IO.Directory]::CreateDirectory($d)|Out-Null
Copy-Item ($s+'\native-system-wnf-subscribe-zero-fresh-v2.cpp') ($d+'\native-system-wnf-subscribe-zero-fresh-v2.cpp')
if((Get-FileHash ($d+'\native-system-wnf-subscribe-zero-fresh-v2.cpp')).Hash.ToLowerInvariant() -ne '5c11293dfabceedf5b6a069ea93f8489df4085a523c7667d8a6289d8a2973740'){throw 'fresh source drift'}
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location $d
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT /GS /we4473 native-system-wnf-subscribe-zero-fresh-v2.cpp /link advapi32.lib /SUBSYSTEM:CONSOLE,10.0 /OUT:native-system-wnf-subscribe-zero-fresh-v2.exe > native-system-wnf-subscribe-zero-fresh-v2-build.log 2>&1'
$code=$LASTEXITCODE
Copy-Item ($d+'\native-system-wnf-subscribe-zero-fresh-v2-build.log') ($s+'\native-system-wnf-subscribe-zero-fresh-v2-build.log')
if($code -ne 0){exit $code}
& ($b+'\python\python.exe') -B ($s+'\inspect-system-wnf-subscribe-pe.py') ($d+'\native-system-wnf-subscribe-zero-fresh-v2.exe') arm64 > ($s+'\native-system-wnf-subscribe-zero-fresh-v2-identity.json')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
foreach($mode in @('value')) {
  & cmd.exe /d /c ("native-system-wnf-subscribe-zero-fresh-v2.exe --owned-"+$mode+" > native-system-wnf-subscribe-zero-fresh-v2-"+$mode+".log 2>&1")
  $code=$LASTEXITCODE
  Copy-Item ($d+'\native-system-wnf-subscribe-zero-fresh-v2-'+$mode+'.log') ($s+'\native-system-wnf-subscribe-zero-fresh-v2-'+$mode+'.log')
  if($code -ne 0){exit $code}
}
exit 0
