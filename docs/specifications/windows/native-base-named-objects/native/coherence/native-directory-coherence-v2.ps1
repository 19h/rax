$ErrorActionPreference='Stop'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$rows=@()
foreach($profile in @('arm64','x64','x86','x86-laa')){
 $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
 $root=$b+'\directory-coherence-v2-'+$profile
 [IO.Directory]::CreateDirectory($root)|Out-Null
 Copy-Item ($s+'\native-directory-coherence-v2.cpp') ($root+'\native-directory-coherence-v2.cpp')
 if((Get-FileHash ($root+'\native-directory-coherence-v2.cpp')).Hash.ToLowerInvariant() -ne 'ff25729e0675ef9e576e7fd651b6bc47428b2d631b2487328213eb13f8002bb5'){throw 'source changed'}
 & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
 Set-Location $root
 $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
 & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /GS /we4129 /we4473 /we4474 /we4477 native-directory-coherence-v2.cpp advapi32.lib /link /SUBSYSTEM:CONSOLE,10.0 /OUT:probe.exe'+$laa+' > build.log 2>&1')
 $build=$LASTEXITCODE
 Copy-Item ($root+'\build.log') ($s+'\directory-coherence-v2-'+$profile+'-build.log')
 if($build -ne 0){exit $build}
 & ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\probe.exe') $profile > ($s+'\directory-coherence-v2-'+$profile+'-before.json')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
 $log='directory-coherence-v2-'+$profile+'.log'
 & cmd.exe /d /c ('probe.exe '+$profile+' > '+$log+' 2>&1')
 $code=$LASTEXITCODE
 Copy-Item ($root+'\'+$log) ($s+'\'+$log)
 $rows+=@{profile=$profile;'exit-code'=$code;log=$log;source=(Get-FileHash ($root+'\native-directory-coherence-v2.cpp')).Hash.ToLowerInvariant()}
 ConvertTo-Json -InputObject @($rows) -Depth 4|Set-Content -Encoding utf8 ($s+'\directory-coherence-v2-outcomes.json')
 & ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\probe.exe') $profile > ($s+'\directory-coherence-v2-'+$profile+'-after.json')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
 if($code -ne 0){exit $code}
}
exit 0
