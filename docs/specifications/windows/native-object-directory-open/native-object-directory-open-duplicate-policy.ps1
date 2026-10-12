$ErrorActionPreference='Stop'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$root='C:\Windows\Temp\assist-native-rax-20261010'
foreach($profile in @('arm64','x64','x86','x86-laa')) {
$b=$root+'\object-directory-open-duplicate-policy-'+$profile
[IO.Directory]::CreateDirectory($b)|Out-Null
Copy-Item -LiteralPath ($share+'\native-object-directory-open-duplicate-policy.cpp') -Destination ($b+'\native-object-directory-open-duplicate-policy.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-object-directory-open-duplicate-policy.cpp')).Hash -ne 'DA20DA30DD0D60002019DF4A6B265480301B65872F081CB7BE37D730A1225A0E'){throw 'source hash mismatch'}
Set-Location $b
$arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
$link=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
$name='native-object-directory-open-duplicate-policy-'+$profile
& cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT native-object-directory-open-duplicate-policy.cpp /link /OUT:'+$name+'.exe'+$link+' > '+$name+'-build.log 2>&1')
if($LASTEXITCODE -ne 0){Get-Content ($name+'-build.log');exit $LASTEXITCODE}
& cmd.exe /d /c ($name+'.exe > '+$name+'.log 2>&1')
$gateExit=$LASTEXITCODE
foreach($suffix in @('.log','-build.log')){Copy-Item -LiteralPath ($b+'\'+$name+$suffix) -Destination ($share+'\'+$name+$suffix);Write-Output ($name+$suffix+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name+$suffix)).Hash)}
if($gateExit -ne 0){exit $gateExit}
}
exit 0
