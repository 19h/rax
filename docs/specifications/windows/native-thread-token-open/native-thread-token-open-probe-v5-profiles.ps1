$ErrorActionPreference='Stop'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$outcomes=@()
foreach($profile in @('arm64','x64','x86','x86-laa')) {
  $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
  $b='C:\Windows\Temp\assist-native-rax-20261010\thread-token-open-'+$profile
  [IO.Directory]::CreateDirectory($b)|Out-Null
  Copy-Item -LiteralPath ($share+'\native-thread-token-open-probe-v5.cpp') -Destination ($b+'\native-thread-token-open-probe-v5.cpp')
  if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-thread-token-open-probe-v5.cpp')).Hash -ne '93179B2BCFEE37AC32ECFB206182ABD5209B9B729261A989D10F68F4BE458453'){throw 'source hash mismatch'}
  Set-Location $b
  & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
  $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
  & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT native-thread-token-open-probe-v5.cpp advapi32.lib /link /OUT:native-thread-token-open-probe-v5-'+$profile+'.exe'+$laa+' > native-thread-token-open-probe-v5-'+$profile+'-build.log 2>&1')
  if($LASTEXITCODE -ne 0){Get-Content ('native-thread-token-open-probe-v5-'+$profile+'-build.log');exit $LASTEXITCODE}
  & cmd.exe /d /c ('native-thread-token-open-probe-v5-'+$profile+'.exe > native-thread-token-open-probe-v5-'+$profile+'.log 2>&1')
  $gateExit=$LASTEXITCODE
  foreach($name in @(('native-thread-token-open-probe-v5-'+$profile+'.log'),('native-thread-token-open-probe-v5-'+$profile+'-build.log'))){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
  $outcomes+=@{'profile'=$profile;'exit-code'=$gateExit;'source-hash'='93179B2BCFEE37AC32ECFB206182ABD5209B9B729261A989D10F68F4BE458453';'exe-sha256'=(Get-FileHash ($b+'\native-thread-token-open-probe-v5-'+$profile+'.exe')).Hash;'exe-bytes'=(Get-Item ($b+'\native-thread-token-open-probe-v5-'+$profile+'.exe')).Length}
  if($gateExit -ne 0){$outcomes|ConvertTo-Json|Set-Content ($share+'\native-thread-token-open-probe-v5-outcomes.json');exit $gateExit}
}
$outcomes|ConvertTo-Json|Set-Content ($share+'\native-thread-token-open-probe-v5-outcomes.json')
