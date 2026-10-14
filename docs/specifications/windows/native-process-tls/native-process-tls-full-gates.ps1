$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010';$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH;$env:RUSTC=$b+'\toolchain\bin\rustc.exe';$env:CARGO_HOME=$b+'\cargo-home';$env:CARGO_TARGET_DIR=$b+'\target-io-completion-tests'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location ($b+'\src')
$results=@()
$steps=@(
 @('full','test --lib'),
 @('capi','test -p rax-capi'),
 @('all-targets','build --all-targets'),
 @('integration','test --test user_windows --test user_windows_memory')
)
foreach($step in $steps){
 $name=$step[0];$parts=$step[1].Split(' ',2);$command='cargo.exe '+$parts[0]+' --locked --offline --no-default-features '+$parts[1]
 if($name -ne 'all-targets'){$command+=' -- --nocapture';if($name -eq 'capi'){$command+=' --test-threads=1'}}
 $log='native-process-tls-full-'+$name+'.log'
 & cmd.exe /d /c ($command+' > '+$b+'\'+$log+' 2>&1')
 $gateExit=$LASTEXITCODE
 Copy-Item -LiteralPath ($b+'\'+$log) -Destination ($s+'\'+$log)
 $results+=@{gate=$name;'exit-code'=$gateExit;command=$command;log=$log}
 $results|ConvertTo-Json -Depth 4|Set-Content -Encoding utf8 -LiteralPath ($s+'\native-process-tls-full-gates.json')
 Write-Output ('gate '+$name+' exit='+$gateExit)
 if($gateExit -ne 0 -and $name -ne 'full'){exit $gateExit}
}
exit 0
