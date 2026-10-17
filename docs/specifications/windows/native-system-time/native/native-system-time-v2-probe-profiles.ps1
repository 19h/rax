$ErrorActionPreference='Stop'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$results=@()
foreach($profile in @('arm64','x64','x86','x86-laa')){
 $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
 $root=$b+'\system-time-v2-'+$profile
 [IO.Directory]::CreateDirectory($root)|Out-Null
 Copy-Item -LiteralPath ($s+'\native-system-time-v2-probe.cpp') -Destination ($root+'\native-system-time-v2-probe.cpp')
 if((Get-FileHash ($root+'\native-system-time-v2-probe.cpp')).Hash.ToLowerInvariant() -ne '7459a8e1a34e9b8a4144a9587af76918f46c23cd2bab926d9966b07e6eda1544'){throw 'source drift'}
 & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
 Set-Location $root
 $stem='native-system-time-v2-probe-'+$profile
 $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
 & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /GS /we4473 native-system-time-v2-probe.cpp /link /SUBSYSTEM:CONSOLE,10.0 /OUT:'+$stem+'.exe'+$laa+' > '+$stem+'-build.log 2>&1')
 $code=$LASTEXITCODE
 Copy-Item ($root+'\'+$stem+'-build.log') ($s+'\'+$stem+'-build.log')
 if($code -ne 0){exit $code}
 & ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\'+$stem+'.exe') $profile > ($s+'\'+$stem+'-identity.json')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
 & cmd.exe /d /c ($stem+'.exe '+$profile+' > '+$stem+'.log 2>&1')
 $code=$LASTEXITCODE
 Copy-Item ($root+'\'+$stem+'.log') ($s+'\'+$stem+'.log')
 $results+=@{profile=$profile;'exit-code'=$code;'source-sha256'='7459a8e1a34e9b8a4144a9587af76918f46c23cd2bab926d9966b07e6eda1544';log=$stem+'.log'}
 $results|ConvertTo-Json -Depth 5|Set-Content -Encoding utf8 ($s+'\native-system-time-v2-probe-outcomes.json')
 if($code -ne 0){exit $code}
}
