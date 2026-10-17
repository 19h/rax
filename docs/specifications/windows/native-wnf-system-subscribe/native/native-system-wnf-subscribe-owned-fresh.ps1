$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$d=$b+'\system-wnf-subscribe-owned-fresh'
[IO.Directory]::CreateDirectory($d)|Out-Null
Copy-Item ($s+'\native-system-wnf-subscribe-owned-fresh.cpp') ($d+'\native-system-wnf-subscribe-owned-fresh.cpp')
if((Get-FileHash ($d+'\native-system-wnf-subscribe-owned-fresh.cpp')).Hash.ToLowerInvariant() -ne '09f0ca55066e8da083fee40ca7d4c7f9c77e300f5c0ec572f5b739cea47b53da'){throw 'fresh source drift'}
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location $d
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT /GS /we4473 native-system-wnf-subscribe-owned-fresh.cpp /link advapi32.lib /SUBSYSTEM:CONSOLE,10.0 /OUT:native-system-wnf-subscribe-owned-fresh.exe > native-system-wnf-subscribe-owned-fresh-build.log 2>&1'
$code=$LASTEXITCODE
Copy-Item ($d+'\native-system-wnf-subscribe-owned-fresh-build.log') ($s+'\native-system-wnf-subscribe-owned-fresh-build.log')
if($code -ne 0){exit $code}
& ($b+'\python\python.exe') -B ($s+'\inspect-system-wnf-subscribe-pe.py') ($d+'\native-system-wnf-subscribe-owned-fresh.exe') arm64 > ($s+'\native-system-wnf-subscribe-owned-fresh-identity.json')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
foreach($mode in @('empty','value')) {
  & cmd.exe /d /c ("native-system-wnf-subscribe-owned-fresh.exe --owned-"+$mode+" > native-system-wnf-subscribe-owned-fresh-"+$mode+".log 2>&1")
  $code=$LASTEXITCODE
  Copy-Item ($d+'\native-system-wnf-subscribe-owned-fresh-'+$mode+'.log') ($s+'\native-system-wnf-subscribe-owned-fresh-'+$mode+'.log')
  if($code -ne 0){exit $code}
}
exit 0
