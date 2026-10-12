$ErrorActionPreference='Stop'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$root='C:\Windows\Temp\assist-native-rax-20261010'
foreach($profile in @('arm64','x64','x86','x86-laa')) {
$b=$root+'\object-directory-open-format-'+$profile
[IO.Directory]::CreateDirectory($b)|Out-Null
Copy-Item -LiteralPath ($share+'\native-object-directory-open-format.cpp') -Destination ($b+'\native-object-directory-open-format.cpp')
if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-object-directory-open-format.cpp')).Hash -ne '7A76A82EE95DAF2EB44A572A7F30FACC2EFA28567DA60878B756B305F3A12FE8'){throw 'source hash mismatch'}
Set-Location $b
$arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
$link=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
$name='native-object-directory-open-format-'+$profile
& cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT native-object-directory-open-format.cpp /link /OUT:'+$name+'.exe'+$link+' > '+$name+'-build.log 2>&1')
if($LASTEXITCODE -ne 0){Get-Content ($name+'-build.log');exit $LASTEXITCODE}
& cmd.exe /d /c ($name+'.exe > '+$name+'.log 2>&1')
$gateExit=$LASTEXITCODE
foreach($suffix in @('.log','-build.log')){Copy-Item -LiteralPath ($b+'\'+$name+$suffix) -Destination ($share+'\'+$name+$suffix);Write-Output ($name+$suffix+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name+$suffix)).Hash)}
if($gateExit -ne 0){exit $gateExit}
}
exit 0
