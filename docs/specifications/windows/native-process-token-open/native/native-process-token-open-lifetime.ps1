$ErrorActionPreference='Stop'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$rows=@()
foreach($profile in @('arm64','x64','x86','x86-laa')){
 $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
 $root=$b+'\process-token-open-lifetime-'+$profile
 [IO.Directory]::CreateDirectory($root)|Out-Null
 Copy-Item ($s+'\native-process-token-open-lifetime.cpp') ($root+'\native-process-token-open-lifetime.cpp')
 if((Get-FileHash ($root+'\native-process-token-open-lifetime.cpp')).Hash.ToLowerInvariant() -ne '02c7e31f0aaedae6f4e44125322de623cc4d1195ad151bf0ffa2f7079ea18180'){throw 'source drift'}
 & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
 Set-Location $root
 $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
 & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /GS /we4473 /we4474 /we4477 native-process-token-open-lifetime.cpp /link /SUBSYSTEM:CONSOLE,10.0 /OUT:probe.exe'+$laa+' > build.log 2>&1')
 if($LASTEXITCODE -ne 0){Copy-Item ($root+'\build.log') ($s+'\process-token-open-lifetime-'+$profile+'-build.log');exit $LASTEXITCODE}
 $log='process-token-open-lifetime-'+$profile+'.log'
 & cmd.exe /d /c ('probe.exe '+$profile+' > '+$log+' 2>&1')
 $code=$LASTEXITCODE
 Copy-Item ($root+'\'+$log) ($s+'\'+$log)
 Copy-Item ($root+'\build.log') ($s+'\process-token-open-lifetime-'+$profile+'-build.log')
 $rows+=@{profile=$profile;'exit-code'=$code;log=$log}
 if($code -ne 0){exit $code}
 & ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\probe.exe') $profile > ($s+'\process-token-open-lifetime-'+$profile+'-identity.json')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
}
ConvertTo-Json -InputObject @($rows) -Depth 4|Set-Content -Encoding utf8 ($s+'\process-token-open-lifetime-outcomes.json')
