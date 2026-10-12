$ErrorActionPreference='Stop'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$root='C:\Windows\Temp\assist-native-rax-20261010'
foreach($profile in @('arm64','x64','x86','x86-laa')) {
$b=$root+'\etw-register-guids-capacity-'+$profile
[IO.Directory]::CreateDirectory($b)|Out-Null
Copy-Item -LiteralPath ($share+'\native-etw-register-guids-capacity.cpp') -Destination ($b+'\native-etw-register-guids-capacity.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-etw-register-guids-capacity.cpp')).Hash -ne 'A0460E16A99BE648EC223EC8E90045732BE722109E0846D69D03238E0BDD41CD'){throw 'source hash mismatch'}
Set-Location $b
$arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
$link=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
$name='native-etw-register-guids-capacity-'+$profile
& cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT native-etw-register-guids-capacity.cpp /link /OUT:'+$name+'.exe'+$link+' > '+$name+'-build.log 2>&1')
if($LASTEXITCODE -ne 0){Get-Content ($name+'-build.log');exit $LASTEXITCODE}
& cmd.exe /d /c ($name+'.exe > '+$name+'.log 2>&1')
$gateExit=$LASTEXITCODE
foreach($suffix in @('.log','-build.log')){Copy-Item -LiteralPath ($b+'\'+$name+$suffix) -Destination ($share+'\'+$name+$suffix);Write-Output ($name+$suffix+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name+$suffix)).Hash)}
if($gateExit -ne 0){exit $gateExit}
}
exit 0
