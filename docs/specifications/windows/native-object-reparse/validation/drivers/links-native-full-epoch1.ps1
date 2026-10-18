$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010';$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
$env:RUSTC=$b+'\toolchain\bin\rustc.exe'
$env:CARGO_HOME=$b+'\cargo-home'
$env:CARGO_TARGET_DIR=$b+'\target-bno-isolation-prototype'
$env:CARGO_BUILD_JOBS='2';$env:CARGO_INCREMENTAL='0'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location ($b+'\src-object-reparse-epoch1')
$results=@()
$steps=@(@('integration','test --test user_windows --test user_windows_memory'),@('capi','test -p rax-capi'),@('full','test --lib'),@('all-targets','check --all-targets'))
foreach($step in $steps){
 $name=$step[0];$parts=$step[1].Split(' ',2)
 $command='cargo.exe '+$parts[0]+' --locked --offline --no-default-features '+$parts[1]
 if($name -ne 'all-targets'){$command+=' -- --nocapture --test-threads=1'}
 $log='links-native-epoch1-'+$name+'.log'
 & cmd.exe /d /c ($command+' > '+$b+'\'+$log+' 2>&1')
 $code=$LASTEXITCODE
 Copy-Item ($b+'\'+$log) ($s+'\'+$log)
 $results+=@{gate=$name;command=$command;log=$log;'exit-code'=$code}
 ConvertTo-Json -InputObject @($results) -Depth 4|Set-Content -Encoding utf8 ($s+'\links-native-epoch1-gates.json')
 Write-Output ('link gate '+$name+' exit='+$code)
 if($code -ne 0 -and $name -ne 'full'){exit $code}
}
exit 0
