$ErrorActionPreference='Stop'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$outcomes=@()
foreach($profile in @('arm64','x64','x86','x86-laa')) {
  $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
  $b='C:\Windows\Temp\assist-native-rax-20261010\scheduler-thread-slot-'+$profile
  [IO.Directory]::CreateDirectory($b)|Out-Null
  Copy-Item -LiteralPath ($share+'\native-scheduler-thread-slot-boundaries-v2.cpp') -Destination ($b+'\native-scheduler-thread-slot-boundaries-v2.cpp')
  if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-scheduler-thread-slot-boundaries-v2.cpp')).Hash -ne '52A7A674F5514267DF0C051114788F04959348779914EA4C2088403259FD8159'){throw 'source hash mismatch'}
  Set-Location $b
  & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
  $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
  & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT native-scheduler-thread-slot-boundaries-v2.cpp /link /OUT:native-scheduler-thread-slot-boundaries-v2-'+$profile+'.exe'+$laa+' > native-scheduler-thread-slot-boundaries-v2-'+$profile+'-build.log 2>&1')
  if($LASTEXITCODE -ne 0){Get-Content ('native-scheduler-thread-slot-boundaries-v2-'+$profile+'-build.log');exit $LASTEXITCODE}
  & cmd.exe /d /c ('native-scheduler-thread-slot-boundaries-v2-'+$profile+'.exe > native-scheduler-thread-slot-boundaries-v2-'+$profile+'.log 2>&1')
  $gateExit=$LASTEXITCODE
  foreach($name in @(('native-scheduler-thread-slot-boundaries-v2-'+$profile+'.log'),('native-scheduler-thread-slot-boundaries-v2-'+$profile+'-build.log'))){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
  $outcomes+=@{'profile'=$profile;'exit-code'=$gateExit;'source-hash'='52A7A674F5514267DF0C051114788F04959348779914EA4C2088403259FD8159';'exe-sha256'=(Get-FileHash ($b+'\native-scheduler-thread-slot-boundaries-v2-'+$profile+'.exe')).Hash;'exe-bytes'=(Get-Item ($b+'\native-scheduler-thread-slot-boundaries-v2-'+$profile+'.exe')).Length}
  if($gateExit -ne 0){$outcomes|ConvertTo-Json|Set-Content ($share+'\native-scheduler-thread-slot-boundaries-v2-outcomes.json');exit $gateExit}
}
$outcomes|ConvertTo-Json|Set-Content ($share+'\native-scheduler-thread-slot-boundaries-v2-outcomes.json')
