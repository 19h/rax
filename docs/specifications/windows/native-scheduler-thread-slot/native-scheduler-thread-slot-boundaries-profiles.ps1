$ErrorActionPreference='Stop'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$outcomes=@()
foreach($profile in @('arm64','x64','x86','x86-laa')) {
  $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
  $b='C:\Windows\Temp\assist-native-rax-20261010\scheduler-thread-slot-'+$profile
  [IO.Directory]::CreateDirectory($b)|Out-Null
  Copy-Item -LiteralPath ($share+'\native-scheduler-thread-slot-boundaries.cpp') -Destination ($b+'\native-scheduler-thread-slot-boundaries.cpp')
  if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-scheduler-thread-slot-boundaries.cpp')).Hash -ne 'E0D95CF4F12D18902E7F1D95046E68D48831E565FB41E521798BB152F7EA2431'){throw 'source hash mismatch'}
  Set-Location $b
  & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
  $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
  & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT native-scheduler-thread-slot-boundaries.cpp /link /OUT:native-scheduler-thread-slot-boundaries-'+$profile+'.exe'+$laa+' > native-scheduler-thread-slot-boundaries-'+$profile+'-build.log 2>&1')
  if($LASTEXITCODE -ne 0){Get-Content ('native-scheduler-thread-slot-boundaries-'+$profile+'-build.log');exit $LASTEXITCODE}
  & cmd.exe /d /c ('native-scheduler-thread-slot-boundaries-'+$profile+'.exe > native-scheduler-thread-slot-boundaries-'+$profile+'.log 2>&1')
  $gateExit=$LASTEXITCODE
  foreach($name in @(('native-scheduler-thread-slot-boundaries-'+$profile+'.log'),('native-scheduler-thread-slot-boundaries-'+$profile+'-build.log'))){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
  $outcomes+=@{'profile'=$profile;'exit-code'=$gateExit;'source-hash'='E0D95CF4F12D18902E7F1D95046E68D48831E565FB41E521798BB152F7EA2431';'exe-sha256'=(Get-FileHash ($b+'\native-scheduler-thread-slot-boundaries-'+$profile+'.exe')).Hash;'exe-bytes'=(Get-Item ($b+'\native-scheduler-thread-slot-boundaries-'+$profile+'.exe')).Length}
  if($gateExit -ne 0){$outcomes|ConvertTo-Json|Set-Content ($share+'\native-scheduler-thread-slot-boundaries-outcomes.json');exit $gateExit}
}
$outcomes|ConvertTo-Json|Set-Content ($share+'\native-scheduler-thread-slot-boundaries-outcomes.json')
