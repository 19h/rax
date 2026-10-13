$ErrorActionPreference='Stop'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$outcomes=@()
foreach($profile in @('arm64','x64','x86','x86-laa')) {
  $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
  $b='C:\Windows\Temp\assist-native-rax-20261010\section-open-'+$profile
  [IO.Directory]::CreateDirectory($b)|Out-Null
  Copy-Item -LiteralPath ($share+'\native-section-open-probe-v3.cpp') -Destination ($b+'\native-section-open-probe-v3.cpp')
  if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-section-open-probe-v3.cpp')).Hash -ne '30CEC3641130887DB7E46F01AE31C592D838F48C8C3BAAEF8FAAF08A4D364A48'){throw 'source hash mismatch'}
  Set-Location $b
  & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
  $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
  & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /we4473 native-section-open-probe-v3.cpp /link /OUT:native-section-open-probe-v3-'+$profile+'.exe'+$laa+' > native-section-open-probe-v3-'+$profile+'-build.log 2>&1')
  if($LASTEXITCODE -ne 0){Get-Content ('native-section-open-probe-v3-'+$profile+'-build.log');exit $LASTEXITCODE}
  & cmd.exe /d /c ('native-section-open-probe-v3-'+$profile+'.exe > native-section-open-probe-v3-'+$profile+'.log 2>&1')
  $gateExit=$LASTEXITCODE
  foreach($name in @(('native-section-open-probe-v3-'+$profile+'.log'),('native-section-open-probe-v3-'+$profile+'-build.log'))){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
  $outcomes+=@{'profile'=$profile;'exit-code'=$gateExit;'source-hash'='30CEC3641130887DB7E46F01AE31C592D838F48C8C3BAAEF8FAAF08A4D364A48';'exe-sha256'=(Get-FileHash ($b+'\native-section-open-probe-v3-'+$profile+'.exe')).Hash;'exe-bytes'=(Get-Item ($b+'\native-section-open-probe-v3-'+$profile+'.exe')).Length}
  if($gateExit -ne 0){$outcomes|ConvertTo-Json|Set-Content ($share+'\native-section-open-probe-v3-outcomes.json');exit $gateExit}
}
$outcomes|ConvertTo-Json|Set-Content ($share+'\native-section-open-probe-v3-outcomes.json')
