$ErrorActionPreference='Stop'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$outcomes=@()
foreach($profile in @('arm64','x64','x86','x86-laa')) {
  $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
  $b='C:\Windows\Temp\assist-native-rax-20261010\section-map-'+$profile
  [IO.Directory]::CreateDirectory($b)|Out-Null
  Copy-Item -LiteralPath ($share+'\native-section-map-collision.cpp') -Destination ($b+'\native-section-map-collision.cpp')
  if((Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\native-section-map-collision.cpp')).Hash -ne 'EB54AE06F894F414D71B29C6BBF522D25A3D9CBDC601ABFC9B998212E9221837'){throw 'source hash mismatch'}
  Set-Location $b
  & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
  $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
  & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /we4473 native-section-map-collision.cpp /link /OUT:native-section-map-collision-'+$profile+'.exe'+$laa+' > native-section-map-collision-'+$profile+'-build.log 2>&1')
  if($LASTEXITCODE -ne 0){Get-Content ('native-section-map-collision-'+$profile+'-build.log');exit $LASTEXITCODE}
  & cmd.exe /d /c ('native-section-map-collision-'+$profile+'.exe > native-section-map-collision-'+$profile+'.log 2>&1')
  $gateExit=$LASTEXITCODE
  foreach($name in @(('native-section-map-collision-'+$profile+'.log'),('native-section-map-collision-'+$profile+'-build.log'))){Copy-Item -LiteralPath ($b+'\'+$name) -Destination ($share+'\'+$name);Write-Output ($name+' '+(Get-FileHash -Algorithm SHA256 -LiteralPath ($b+'\'+$name)).Hash)}
  $outcomes+=@{'profile'=$profile;'exit-code'=$gateExit;'source-hash'='EB54AE06F894F414D71B29C6BBF522D25A3D9CBDC601ABFC9B998212E9221837';'exe-sha256'=(Get-FileHash ($b+'\native-section-map-collision-'+$profile+'.exe')).Hash;'exe-bytes'=(Get-Item ($b+'\native-section-map-collision-'+$profile+'.exe')).Length}
  if($gateExit -ne 0){$outcomes|ConvertTo-Json|Set-Content ($share+'\native-section-map-collision-outcomes.json');exit $gateExit}
}
$outcomes|ConvertTo-Json|Set-Content ($share+'\native-section-map-collision-outcomes.json')
