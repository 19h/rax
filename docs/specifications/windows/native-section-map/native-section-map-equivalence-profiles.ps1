$ErrorActionPreference='Stop'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$outcomes=@()
foreach($profile in @('arm64','x64','x86','x86-laa')) {
  $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
  $b='C:\Windows\Temp\assist-native-rax-20261010\section-map-'+$profile
  [IO.Directory]::CreateDirectory($b)|Out-Null
  Copy-Item -LiteralPath ($share+'\native-section-map-equivalence.cpp') -Destination ($b+'\native-section-map-equivalence.cpp')
  if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-section-map-equivalence.cpp')).Hash -ne 'A6EB1A5C9F6AB9F355BDE205F01499F6D57073D848080E42B3384C8180874B27'){throw 'source hash mismatch'}
  Set-Location $b
  & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
  $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
  & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /we4473 native-section-map-equivalence.cpp bcrypt.lib /link /OUT:native-section-map-equivalence-'+$profile+'.exe'+$laa+' > native-section-map-equivalence-'+$profile+'-build.log 2>&1')
  if($LASTEXITCODE -ne 0){Get-Content ('native-section-map-equivalence-'+$profile+'-build.log');exit $LASTEXITCODE}
  & cmd.exe /d /c ('native-section-map-equivalence-'+$profile+'.exe > native-section-map-equivalence-'+$profile+'.log 2>&1')
  $gateExit=$LASTEXITCODE
  foreach($name in @(('native-section-map-equivalence-'+$profile+'.log'),('native-section-map-equivalence-'+$profile+'-build.log'))){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
  $outcomes+=@{'profile'=$profile;'exit-code'=$gateExit;'source-hash'='A6EB1A5C9F6AB9F355BDE205F01499F6D57073D848080E42B3384C8180874B27';'exe-sha256'=(Get-FileHash ($b+'\native-section-map-equivalence-'+$profile+'.exe')).Hash;'exe-bytes'=(Get-Item ($b+'\native-section-map-equivalence-'+$profile+'.exe')).Length}
  if($gateExit -ne 0){$outcomes|ConvertTo-Json|Set-Content ($share+'\native-section-map-equivalence-outcomes.json');exit $gateExit}
}
$outcomes|ConvertTo-Json|Set-Content ($share+'\native-section-map-equivalence-outcomes.json')
