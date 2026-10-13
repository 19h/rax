$ErrorActionPreference='Stop'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$outcomes=@()
foreach($profile in @('arm64','x64','x86','x86-laa')) {
  $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
  $b='C:\Windows\Temp\assist-native-rax-20261010\image-extension-'+$profile
  [IO.Directory]::CreateDirectory($b)|Out-Null
  Copy-Item -LiteralPath ($share+'\native-image-extension-limits.cpp') -Destination ($b+'\native-image-extension-limits.cpp')
  if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-image-extension-limits.cpp')).Hash -ne 'CA452626A697BC1D6A63DC10BEDB9451B3F4700A0C4AFB792A7039FFE25D926F'){throw 'source hash mismatch'}
  if($profile -eq 'arm64') {
    foreach($tail in @('.log','-build.log','.exe')) {
      $prior=$b+'\native-image-extension-limits-arm64'+$tail
      if(Test-Path -LiteralPath $prior){Move-Item -LiteralPath $prior -Destination ($b+'\diagnostic-first-native-image-extension-limits-arm64'+$tail)}
    }
  }
  Set-Location $b
  & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
  $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
  & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT native-image-extension-limits.cpp /link /OUT:native-image-extension-limits-'+$profile+'.exe'+$laa+' > native-image-extension-limits-'+$profile+'-build.log 2>&1')
  if($LASTEXITCODE -ne 0){Get-Content ('native-image-extension-limits-'+$profile+'-build.log');exit $LASTEXITCODE}
  & cmd.exe /d /c ('native-image-extension-limits-'+$profile+'.exe > native-image-extension-limits-'+$profile+'.log 2>&1')
  $gateExit=$LASTEXITCODE
  foreach($name in @(('native-image-extension-limits-'+$profile+'.log'),('native-image-extension-limits-'+$profile+'-build.log'))){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
  $outcomes+=@{'profile'=$profile;'exit-code'=$gateExit;'source-hash'='CA452626A697BC1D6A63DC10BEDB9451B3F4700A0C4AFB792A7039FFE25D926F';'exe-sha256'=(Get-FileHash ($b+'\native-image-extension-limits-'+$profile+'.exe')).Hash;'exe-bytes'=(Get-Item ($b+'\native-image-extension-limits-'+$profile+'.exe')).Length}
  if($gateExit -ne 0){$outcomes|ConvertTo-Json|Set-Content ($share+'\native-image-extension-limits-outcomes.json');exit $gateExit}
}
$outcomes|ConvertTo-Json|Set-Content ($share+'\native-image-extension-limits-outcomes.json')
