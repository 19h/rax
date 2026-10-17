$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$d=$b+'\system-wnf-subscribe-zero-fresh'
[IO.Directory]::CreateDirectory($d)|Out-Null
Copy-Item ($s+'\native-system-wnf-subscribe-zero-fresh.cpp') ($d+'\native-system-wnf-subscribe-zero-fresh.cpp')
if((Get-FileHash ($d+'\native-system-wnf-subscribe-zero-fresh.cpp')).Hash.ToLowerInvariant() -ne 'b7b290d1f5ae3b4a5de39946f6ee0c5da05290807356949a50b6ae3f3d1af982'){throw 'fresh source drift'}
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location $d
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT /GS /we4473 native-system-wnf-subscribe-zero-fresh.cpp /link advapi32.lib /SUBSYSTEM:CONSOLE,10.0 /OUT:native-system-wnf-subscribe-zero-fresh.exe > native-system-wnf-subscribe-zero-fresh-build.log 2>&1'
$code=$LASTEXITCODE
Copy-Item ($d+'\native-system-wnf-subscribe-zero-fresh-build.log') ($s+'\native-system-wnf-subscribe-zero-fresh-build.log')
if($code -ne 0){exit $code}
& ($b+'\python\python.exe') -B ($s+'\inspect-system-wnf-subscribe-pe.py') ($d+'\native-system-wnf-subscribe-zero-fresh.exe') arm64 > ($s+'\native-system-wnf-subscribe-zero-fresh-identity.json')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
foreach($mode in @('value')) {
  & cmd.exe /d /c ("native-system-wnf-subscribe-zero-fresh.exe --owned-"+$mode+" > native-system-wnf-subscribe-zero-fresh-"+$mode+".log 2>&1")
  $code=$LASTEXITCODE
  Copy-Item ($d+'\native-system-wnf-subscribe-zero-fresh-'+$mode+'.log') ($s+'\native-system-wnf-subscribe-zero-fresh-'+$mode+'.log')
  if($code -ne 0){exit $code}
}
exit 0
