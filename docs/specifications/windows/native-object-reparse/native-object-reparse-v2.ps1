$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010';$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$modules=@('C:\Windows\System32\ntdll.dll','C:\Windows\SysWOW64\ntdll.dll')
Get-FileHash $modules|ConvertTo-Json|Set-Content -Encoding utf8 ($s+'\native-object-reparse-v2-modules-before.json')
$rows=@();$failed=$false
foreach($profile in @('arm64','x64','x86','x86-laa')){
 $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
 $root=$b+'\native-object-reparse-v2-'+$profile
 [IO.Directory]::CreateDirectory($root)|Out-Null
 Copy-Item ($s+'\native-object-reparse-v2.cpp') ($root+'\native-object-reparse-v2.cpp')
 if((Get-FileHash ($root+'\native-object-reparse-v2.cpp')).Hash.ToLowerInvariant() -ne 'a8ea0766aa6d744dc2155d053a694fd9b5111cbe07d59574d2162992fa327204'){throw 'native source changed'}
 & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
 Set-Location $root
 $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
 & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /GS /we4473 /we4474 /we4477 native-object-reparse-v2.cpp /link /SUBSYSTEM:CONSOLE,10.0 /OUT:probe.exe'+$laa+' > build.log 2>&1')
 Copy-Item ($root+'\build.log') ($s+'\native-object-reparse-v2-'+$profile+'-build.log')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
 & ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\probe.exe') $profile > ($s+'\native-object-reparse-v2-'+$profile+'-before.json')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
 $log='native-object-reparse-v2-'+$profile+'.log'
 & cmd.exe /d /c ('probe.exe '+$profile+' > '+$log+' 2>&1')
 $code=$LASTEXITCODE
 if($code -ne 0){$failed=$true}
 Copy-Item ($root+'\'+$log) ($s+'\'+$log)
 $rows+=@{profile=$profile;'exit-code'=$code;log=$log}
 ConvertTo-Json -InputObject @($rows) -Depth 4|Set-Content -Encoding utf8 ($s+'\native-object-reparse-v2-outcomes.json')
 & ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\probe.exe') $profile > ($s+'\native-object-reparse-v2-'+$profile+'-after.json')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
 (Get-FileHash ($root+'\native-object-reparse-v2.cpp'))|ConvertTo-Json|Set-Content -Encoding utf8 ($s+'\native-object-reparse-v2-'+$profile+'-source-after.json')
}
Get-FileHash $modules|ConvertTo-Json|Set-Content -Encoding utf8 ($s+'\native-object-reparse-v2-modules-after.json')

if($failed){exit 1}
