$ErrorActionPreference='Stop'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$root=$b+'\bno-copy-curve-controller'
[IO.Directory]::CreateDirectory($root)|Out-Null
Copy-Item ($s+'\native-bno-copy-curve-child.cpp') ($root+'\native-bno-copy-curve-child.cpp')
if((Get-FileHash ($root+'\native-bno-copy-curve-child.cpp')).Hash.ToLowerInvariant() -ne 'fb6ac40384206b54bb0ea7f1050b1130d3e0bcf3a8bce8f28d95a6cc906d3149'){throw 'source drift'}
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location $root
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT /GS /we4473 /we4474 /we4477 native-bno-copy-curve-child.cpp /link /SUBSYSTEM:CONSOLE,10.0 /OUT:controller.exe > build.log 2>&1'
Copy-Item ($root+'\build.log') ($s+'\bno-copy-curve-build.log')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\controller.exe') arm64 > ($s+'\bno-copy-curve-before.json')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
$rows=@()
foreach($profile in @('x86','x86-laa')){
 foreach($length in @(8,15,16,17,31,32,33,63,64,65,66,67,127,128,129,134,135)){
  $log='bno-copy-curve-'+$profile+'-'+$length+'.log'
  & cmd.exe /d /c ('controller.exe '+$b+'\bno-copy-'+$profile+'\probe.exe '+$profile+' enabled n'+$length+' > '+$log+' 2>&1')
  $code=$LASTEXITCODE
  Copy-Item ($root+'\'+$log) ($s+'\'+$log)
  $rows+=@{profile=$profile;length=$length;'exit-code'=$code;log=$log}
  ConvertTo-Json -InputObject @($rows) -Depth 4|Set-Content -Encoding utf8 ($s+'\bno-copy-curve-outcomes.json')
 }
}
& ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\controller.exe') arm64 > ($s+'\bno-copy-curve-after.json')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
