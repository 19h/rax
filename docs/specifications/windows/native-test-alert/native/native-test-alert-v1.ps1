$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$workspace=$b+'\probe-src-test-alert-v1'
if(Test-Path $workspace){throw 'probe source workspace already exists'}
[IO.Directory]::CreateDirectory($workspace)|Out-Null
Copy-Item ($s+'\native-test-alert-v1.cpp') ($workspace+'\native-test-alert-v1.cpp')
if((Get-FileHash ($workspace+'\native-test-alert-v1.cpp')).Hash.ToLowerInvariant() -ne '6393a325c24f1b1c93bed00b05d8d4ecb54fab80693962f3bc278560f0fe9172'){throw 'native source changed'}
$modules=@('C:\Windows\System32\ntdll.dll','C:\Windows\SysWOW64\ntdll.dll','C:\Windows\System32\KernelBase.dll','C:\Windows\SysWOW64\KernelBase.dll','C:\Windows\System32\kernel32.dll','C:\Windows\SysWOW64\kernel32.dll')
Get-FileHash $modules|ConvertTo-Json|Set-Content -Encoding utf8 ($s+'\native-test-alert-v1-modules-before.json')
$rows=@()
foreach($profile in @('arm64','x64','x86','x86-laa')){
 $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
 $out=$workspace+'\target\'+$profile
 [IO.Directory]::CreateDirectory($out)|Out-Null
 [IO.Directory]::CreateDirectory($workspace+'\target\tmp')|Out-Null
 $env:TMP=$workspace+'\target\tmp'
 $env:TEMP=$env:TMP
 & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
 Set-Location $out
 $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
 & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /EHsc /MT /GS /Od /we4473 /we4474 /we4477 '+$workspace+'\native-test-alert-v1.cpp /link /SUBSYSTEM:CONSOLE,10.0 /OUT:probe.exe'+$laa+' > build.log 2>&1')
 Copy-Item ($out+'\build.log') ($s+'\native-test-alert-v1-'+$profile+'-build.log')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
 & ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($out+'\probe.exe') $profile > ($s+'\native-test-alert-v1-'+$profile+'-before.json')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
 $log='native-test-alert-v1-'+$profile+'.log'
 & cmd.exe /d /c ('probe.exe '+$profile+' > '+$log+' 2>&1')
 $code=$LASTEXITCODE
 Copy-Item ($out+'\'+$log) ($s+'\'+$log)
 $rows+=@{profile=$profile;'exit-code'=$code;log=$log}
 ConvertTo-Json -InputObject @($rows) -Depth 4|Set-Content -Encoding utf8 ($s+'\native-test-alert-v1-outcomes.json')
 & ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($out+'\probe.exe') $profile > ($s+'\native-test-alert-v1-'+$profile+'-after.json')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
 Get-FileHash ($workspace+'\native-test-alert-v1.cpp')|ConvertTo-Json|Set-Content -Encoding utf8 ($s+'\native-test-alert-v1-'+$profile+'-source-after.json')
 if($code -ne 0){exit $code}
}
Get-FileHash $modules|ConvertTo-Json|Set-Content -Encoding utf8 ($s+'\native-test-alert-v1-modules-after.json')
