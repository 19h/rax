$ErrorActionPreference='Stop'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$outcomes=@()
foreach($profile in @('arm64','x64','x86','x86-laa')) {
  $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
  $b='C:\Windows\Temp\assist-native-rax-20261010\scheduler-shared-'+$profile
  [IO.Directory]::CreateDirectory($b)|Out-Null
  Copy-Item -LiteralPath ($share+'\native-scheduler-shared-create.cpp') -Destination ($b+'\native-scheduler-shared-create.cpp')
  if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-scheduler-shared-create.cpp')).Hash -ne '49D117CA5DF518465B494DED5AD2B63EEF77B3FAD17CB88047D7786DB6BB560A'){throw 'source hash mismatch'}
  if($profile -eq 'arm64') {
    foreach($tail in @('.log','-build.log','.exe')) {
      $prior=$b+'\native-scheduler-shared-create-arm64'+$tail
      if(Test-Path -LiteralPath $prior){Move-Item -LiteralPath $prior -Destination ($b+'\diagnostic-first-native-scheduler-shared-create-arm64'+$tail)}
    }
  }
  Set-Location $b
  & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
  $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
  & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT native-scheduler-shared-create.cpp /link /OUT:native-scheduler-shared-create-'+$profile+'.exe'+$laa+' > native-scheduler-shared-create-'+$profile+'-build.log 2>&1')
  if($LASTEXITCODE -ne 0){Get-Content ('native-scheduler-shared-create-'+$profile+'-build.log');exit $LASTEXITCODE}
  & cmd.exe /d /c ('native-scheduler-shared-create-'+$profile+'.exe > native-scheduler-shared-create-'+$profile+'.log 2>&1')
  $gateExit=$LASTEXITCODE
  foreach($name in @(('native-scheduler-shared-create-'+$profile+'.log'),('native-scheduler-shared-create-'+$profile+'-build.log'))){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
  $outcomes+=@{'profile'=$profile;'exit-code'=$gateExit;'source-hash'='49D117CA5DF518465B494DED5AD2B63EEF77B3FAD17CB88047D7786DB6BB560A';'exe-sha256'=(Get-FileHash ($b+'\native-scheduler-shared-create-'+$profile+'.exe')).Hash;'exe-bytes'=(Get-Item ($b+'\native-scheduler-shared-create-'+$profile+'.exe')).Length}
  if($gateExit -ne 0){$outcomes|ConvertTo-Json|Set-Content ($share+'\native-scheduler-shared-create-outcomes.json');exit $gateExit}
}
$outcomes|ConvertTo-Json|Set-Content ($share+'\native-scheduler-shared-create-outcomes.json')
