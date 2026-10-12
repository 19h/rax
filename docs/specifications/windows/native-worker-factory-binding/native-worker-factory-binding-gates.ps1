$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010';$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH;$env:RUSTC=$b+'\toolchain\bin\rustc.exe';$env:CARGO_HOME=$b+'\cargo-home';$env:CARGO_TARGET_DIR=$b+'\target-io-completion-tests'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location ($b+'\src')
$results=@()
$steps=@(
 @('targeted','test --lib worker_factory_binding'),
 @('soft-maximum','test --lib worker_factory_soft_maximum'),
 @('hard-maximum','test --lib worker_factory_set_maximum'),
 @('wait-packet-associate','test --lib wait_packet_associate'),
 @('wait-packet-create','test --lib wait_packet_create'),
 @('threading','test --lib dll::threading::tests'),
 @('timer2','test --lib timer2_create'),
 @('worker-factory','test --lib worker_factory_create'),
 @('events','test --lib event_'),
 @('io-completion','test --lib io_completion'),
 @('full','test --lib'),
 @('capi','test -p rax-capi'),
 @('all-targets','build --all-targets'),
 @('integration','test --test user_windows_memory')
)
foreach($step in $steps){
 $name=$step[0];$parts=$step[1].Split(' ',2);$command='cargo.exe '+$parts[0]+' --locked --offline --no-default-features '+$parts[1]
 if($name -ne 'all-targets'){$command+=' -- --nocapture'}
 $log='native-worker-factory-binding-'+$name+'.log'
 & cmd.exe /d /c ($command+' > '+$b+'\'+$log+' 2>&1')
 $gateExit=$LASTEXITCODE
 Copy-Item -LiteralPath ($b+'\'+$log) -Destination ($s+'\'+$log)
 $results+=@{gate=$name;'exit-code'=$gateExit;command=$command;log=$log}
 $results|ConvertTo-Json -Depth 4|Set-Content -Encoding utf8 -LiteralPath ($s+'\native-worker-factory-binding-gates.json')
 Write-Output ('gate '+$name+' exit='+$gateExit)
 if($gateExit -ne 0 -and $name -ne 'full'){exit $gateExit}
}
exit 0
