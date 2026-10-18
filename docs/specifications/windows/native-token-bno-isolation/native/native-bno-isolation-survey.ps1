$ErrorActionPreference='Stop'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$rows=@()
foreach($profile in @('arm64','x64','x86','x86-laa')){
 $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
 $root=$b+'\bno-isolation-survey-'+$profile
 [IO.Directory]::CreateDirectory($root)|Out-Null
 Copy-Item ($s+'\native-bno-isolation-survey.cpp') ($root+'\native-bno-isolation-survey.cpp')
 if((Get-FileHash ($root+'\native-bno-isolation-survey.cpp')).Hash.ToLowerInvariant() -ne '7e5c5a7a8b05128140c0c4c61a0398767407157a7459993ff6d8a8f09ce27565'){throw 'source drift'}
 & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
 Set-Location $root
 $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
 & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /GS /we4473 /we4474 /we4477 native-bno-isolation-survey.cpp advapi32.lib /link /SUBSYSTEM:CONSOLE,10.0 /OUT:probe.exe'+$laa+' > build.log 2>&1')
 if($LASTEXITCODE -ne 0){Copy-Item ($root+'\build.log') ($s+'\bno-isolation-survey-'+$profile+'-build.log');exit $LASTEXITCODE}
 foreach($class in @(44)){
  & ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\probe.exe') $profile > ($s+'\bno-isolation-survey-'+$profile+'-identity-before.json')
  if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
  $log='bno-isolation-survey-'+$profile+'-class'+$class+'.log'
  & cmd.exe /d /c ('probe.exe '+$profile+' '+$class+' > '+$log+' 2>&1')
  $code=$LASTEXITCODE
  Copy-Item ($root+'\'+$log) ($s+'\'+$log)
  $rows+=@{profile=$profile;class=$class;'exit-code'=$code;log=$log}
 }
 Copy-Item ($root+'\build.log') ($s+'\bno-isolation-survey-'+$profile+'-build.log')
 & ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\probe.exe') $profile > ($s+'\bno-isolation-survey-'+$profile+'-identity.json')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
}
ConvertTo-Json -InputObject @($rows) -Depth 4|Set-Content -Encoding utf8 ($s+'\bno-isolation-survey-outcomes.json')
