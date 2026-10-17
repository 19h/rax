$ErrorActionPreference='Stop'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$rows=@()
foreach($profile in @('arm64','x64','x86','x86-laa')){
 $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
 $root=$b+'\dynamic-table-lifetime-'+$profile
 [IO.Directory]::CreateDirectory($root)|Out-Null
 Copy-Item ($s+'\native-dynamic-table-lifetime.cpp') ($root+'\native-dynamic-table-lifetime.cpp')
 if((Get-FileHash ($root+'\native-dynamic-table-lifetime.cpp')).Hash.ToLowerInvariant() -ne '0f7c87a682fd811b971f01ba84ca3a05fd9467e62d85b5e819dcc0860146f826'){throw 'source drift'}
 & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
 Set-Location $root
 $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
 & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /GS /we4473 native-dynamic-table-lifetime.cpp /link /SUBSYSTEM:CONSOLE,10.0 /OUT:probe.exe'+$laa+' > build.log 2>&1')
 if($LASTEXITCODE -ne 0){Copy-Item ($root+'\build.log') ($s+'\dynamic-table-lifetime-'+$profile+'-build.log');exit $LASTEXITCODE}
 foreach($mode in 0..0){
  $log='dynamic-table-lifetime-'+$profile+'-'+$mode+'.log'
  & cmd.exe /d /c ('probe.exe '+$profile+' > '+$log+' 2>&1')
  $code=$LASTEXITCODE
  Copy-Item ($root+'\'+$log) ($s+'\'+$log)
  $rows+=@{profile=$profile;mode=$mode;'exit-code'=$code;log=$log}
 }
 & ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\probe.exe') $profile > ($s+'\dynamic-table-lifetime-'+$profile+'-identity.json')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
}
ConvertTo-Json -InputObject @($rows) -Depth 4|Set-Content -Encoding utf8 ($s+'\dynamic-table-lifetime-outcomes.json')
