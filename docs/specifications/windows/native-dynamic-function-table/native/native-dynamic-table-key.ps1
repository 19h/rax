$ErrorActionPreference='Stop'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$rows=@()
foreach($profile in @('arm64','x64','x86','x86-laa')){
 $arch=if($profile -eq 'x86-laa'){'x86'}else{$profile}
 $root=$b+'\dynamic-table-key-'+$profile
 [IO.Directory]::CreateDirectory($root)|Out-Null
 Copy-Item ($s+'\native-dynamic-table-key.cpp') ($root+'\native-dynamic-table-key.cpp')
 if((Get-FileHash ($root+'\native-dynamic-table-key.cpp')).Hash.ToLowerInvariant() -ne '33d96600e526e065b4e0ef655a36cc4e66851bdca76c6ad23399a8e45d246eef'){throw 'source drift'}
 & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $arch -HostArch arm64
 Set-Location $root
 $laa=if($profile -eq 'x86-laa'){' /LARGEADDRESSAWARE'}else{''}
 & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /GS /we4473 native-dynamic-table-key.cpp /link /SUBSYSTEM:CONSOLE,10.0 /OUT:probe.exe'+$laa+' > build.log 2>&1')
 if($LASTEXITCODE -ne 0){Copy-Item ($root+'\build.log') ($s+'\dynamic-table-key-'+$profile+'-build.log');exit $LASTEXITCODE}
 foreach($mode in 0..0){
  $log='dynamic-table-key-'+$profile+'-'+$mode+'.log'
  & cmd.exe /d /c ('probe.exe '+$profile+' > '+$log+' 2>&1')
  $code=$LASTEXITCODE
  Copy-Item ($root+'\'+$log) ($s+'\'+$log)
  $rows+=@{profile=$profile;mode=$mode;'exit-code'=$code;log=$log}
 }
 & ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($root+'\probe.exe') $profile > ($s+'\dynamic-table-key-'+$profile+'-identity.json')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
}
ConvertTo-Json -InputObject @($rows) -Depth 4|Set-Content -Encoding utf8 ($s+'\dynamic-table-key-outcomes.json')
