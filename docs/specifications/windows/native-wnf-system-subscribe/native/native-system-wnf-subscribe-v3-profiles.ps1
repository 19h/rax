$ErrorActionPreference='Stop'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$results=@()
foreach($profile in @('arm64','x64','x86','x86-laa')){
 $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
 $root=$b+'\system-wnf-subscribe-'+$profile
 [IO.Directory]::CreateDirectory($root)|Out-Null
 Copy-Item -LiteralPath ($s+'\native-system-wnf-subscribe-v3.cpp') -Destination ($root+'\native-system-wnf-subscribe-v3.cpp')
 if((Get-FileHash ($root+'\native-system-wnf-subscribe-v3.cpp')).Hash.ToLowerInvariant() -ne 'edd443e8b06fa244b65d5226309818767675b4af139cf44d1273a65aeb4d8c9a'){throw 'source drift'}
 & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
 Set-Location $root
 $stem='native-system-wnf-subscribe-v3-'+$profile
 $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
 & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /GS /we4473 native-system-wnf-subscribe-v3.cpp advapi32.lib /link /SUBSYSTEM:CONSOLE,10.0 /OUT:'+$stem+'.exe'+$laa+' > '+$stem+'-build.log 2>&1')
 $code=$LASTEXITCODE
 Copy-Item ($root+'\'+$stem+'-build.log') ($s+'\'+$stem+'-build.log')
 if($code -ne 0){exit $code}
 & ($b+'\python\python.exe') -B ($s+'\inspect-system-wnf-subscribe-pe.py') ($root+'\'+$stem+'.exe') $profile > ($s+'\'+$stem+'-identity.json')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
 & cmd.exe /d /c ($stem+'.exe '+$profile+' > '+$stem+'.log 2>&1')
 $code=$LASTEXITCODE
 Copy-Item ($root+'\'+$stem+'.log') ($s+'\'+$stem+'.log')
 $results+=@{profile=$profile;'exit-code'=$code;'source-sha256'='edd443e8b06fa244b65d5226309818767675b4af139cf44d1273a65aeb4d8c9a';log=$stem+'.log'}
 $results|ConvertTo-Json -Depth 5|Set-Content -Encoding utf8 ($s+'\native-system-wnf-subscribe-v3-outcomes.json')
 if($code -ne 0){exit $code}
}
