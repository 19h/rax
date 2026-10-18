$ErrorActionPreference='Stop'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$control=$b+'\bno-copy-controller'
[IO.Directory]::CreateDirectory($control)|Out-Null
Copy-Item ($s+'\native-bno-copy-child.cpp') ($control+'\native-bno-copy-child.cpp')
if((Get-FileHash ($control+'\native-bno-copy-child.cpp')).Hash.ToLowerInvariant() -ne '0c8c05094853ef5f7aeaa71565dbf1b03a22b40c573fdf95bb533f702d543ba8'){throw 'controller source drift'}
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location $control
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT /GS /we4473 /we4474 /we4477 native-bno-copy-child.cpp /link /SUBSYSTEM:CONSOLE,10.0 /OUT:controller.exe > build.log 2>&1'
Copy-Item ($control+'\build.log') ($s+'\bno-copy-controller-build.log')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($control+'\controller.exe') arm64 > ($s+'\bno-copy-controller-before.json')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
$rows=@()
foreach($profile in @('arm64','x64','x86','x86-laa')){
 $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
 $root=$b+'\bno-copy-'+$profile
 [IO.Directory]::CreateDirectory($root)|Out-Null
 Copy-Item ($s+'\native-bno-copy-boundaries.cpp') ($root+'\native-bno-copy-boundaries.cpp')
 if((Get-FileHash ($root+'\native-bno-copy-boundaries.cpp')).Hash.ToLowerInvariant() -ne '83189ac043b406bcac2782e05bf14e3556c16d80ed8898aff7620309238d657e'){throw 'probe source drift'}
 & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
 Set-Location $root
 $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
 & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /GS /we4473 /we4474 /we4477 native-bno-copy-boundaries.cpp advapi32.lib /link /SUBSYSTEM:CONSOLE,10.0 /OUT:probe.exe'+$laa+' > build.log 2>&1')
 Copy-Item ($root+'\build.log') ($s+'\bno-copy-'+$profile+'-build.log')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
 & ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\probe.exe') $profile > ($s+'\bno-copy-'+$profile+'-before.json')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
 foreach($mode in @('ascii','unicode','long')){
  $log='bno-copy-'+$profile+'-'+$mode+'.log'
  & cmd.exe /d /c ($control+'\controller.exe '+$root+'\probe.exe '+$profile+' enabled '+$mode+' > '+$log+' 2>&1')
  $code=$LASTEXITCODE
  Copy-Item ($root+'\'+$log) ($s+'\'+$log)
  $rows+=@{profile=$profile;mode=$mode;'exit-code'=$code;log=$log}
  ConvertTo-Json -InputObject @($rows) -Depth 4|Set-Content -Encoding utf8 ($s+'\bno-copy-outcomes.json')
 }
 & ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\probe.exe') $profile > ($s+'\bno-copy-'+$profile+'-after.json')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
}
& ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($control+'\controller.exe') arm64 > ($s+'\bno-copy-controller-after.json')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
