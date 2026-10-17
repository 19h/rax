$ErrorActionPreference='Stop'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$results=@()
foreach($profile in @('arm64','x64','x86','x86-laa')){
 $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
 $root=$b+'\image-extension-owner-v3-'+$profile
 [IO.Directory]::CreateDirectory($root)|Out-Null
 Copy-Item -LiteralPath ($s+'\native-image-extension-owner-probe-v3.cpp') -Destination ($root+'\native-image-extension-owner-probe-v3.cpp')
 if((Get-FileHash ($root+'\native-image-extension-owner-probe-v3.cpp')).Hash.ToLowerInvariant() -ne 'ca9e1ad7e97200e2dd6c793e0d56875b271f36fa32fbfaf147782ae370943092'){throw 'source drift'}
 & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
 Set-Location $root
 $stem='native-image-extension-owner-probe-v3-'+$profile
 $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
 & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /GS /we4473 native-image-extension-owner-probe-v3.cpp /link /SUBSYSTEM:CONSOLE,10.0 /OUT:'+$stem+'.exe'+$laa+' > '+$stem+'-build.log 2>&1')
 $code=$LASTEXITCODE
 Copy-Item ($root+'\'+$stem+'-build.log') ($s+'\'+$stem+'-build.log')
 if($code -ne 0){exit $code}
 & ($b+'\python\python.exe') -B ($s+'\inspect-image-extension-owner-pe.py') ($root+'\'+$stem+'.exe') $profile > ($s+'\'+$stem+'-identity.json')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
 & cmd.exe /d /c ($stem+'.exe '+$profile+' > '+$stem+'.log 2>&1')
 $code=$LASTEXITCODE
 Copy-Item ($root+'\'+$stem+'.log') ($s+'\'+$stem+'.log')
 $results+=@{profile=$profile;'exit-code'=$code;'source-sha256'='ca9e1ad7e97200e2dd6c793e0d56875b271f36fa32fbfaf147782ae370943092';log=$stem+'.log'}
 $results|ConvertTo-Json -Depth 5|Set-Content -Encoding utf8 ($s+'\native-image-extension-owner-probe-v3-outcomes.json')
 if($code -ne 0){exit $code}
}
