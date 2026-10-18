$ErrorActionPreference='Stop'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$root=$b+'\bno-isolation-child'
[IO.Directory]::CreateDirectory($root)|Out-Null
Copy-Item ($s+'\native-bno-isolation-child.cpp') ($root+'\native-bno-isolation-child.cpp')
if((Get-FileHash ($root+'\native-bno-isolation-child.cpp')).Hash.ToLowerInvariant() -ne 'f6b8a02161abdc141cf3c1b8cc09dd9cf1f6824fe5e4ad14b9a69dfa0c4abb9a'){throw 'source drift'}
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location $root
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT /GS /we4473 /we4474 /we4477 native-bno-isolation-child.cpp /link /SUBSYSTEM:CONSOLE,10.0 /OUT:child-control.exe > build.log 2>&1'
Copy-Item ($root+'\build.log') ($s+'\bno-isolation-child-build.log')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\child-control.exe') arm64 > ($s+'\bno-isolation-child-identity-before.json')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
$rows=@()
foreach($profile in @('arm64','x64','x86','x86-laa')){
 foreach($mode in @('disabled-empty','enabled-ascii','enabled-unicode')){
  $parts=$mode.Split('-');$log='bno-isolation-child-'+$profile+'-'+$mode+'.log'
  $probe=$b+'\bno-isolation-survey-'+$profile+'\probe.exe'
  & cmd.exe /d /c ('child-control.exe '+$probe+' '+$profile+' '+$parts[0]+' '+$parts[1]+' > '+$log+' 2>&1')
  $code=$LASTEXITCODE
  Copy-Item ($root+'\'+$log) ($s+'\'+$log)
  $rows+=@{profile=$profile;mode=$mode;'exit-code'=$code;log=$log}
  ConvertTo-Json -InputObject @($rows) -Depth 4|Set-Content -Encoding utf8 ($s+'\bno-isolation-child-outcomes.json')
 }
}
& ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\child-control.exe') arm64 > ($s+'\bno-isolation-child-identity-after.json')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
