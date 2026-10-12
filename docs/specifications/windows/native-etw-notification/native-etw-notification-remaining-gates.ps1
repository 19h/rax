$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010';$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH;$env:RUSTC=$b+'\toolchain\bin\rustc.exe';$env:CARGO_HOME=$b+'\cargo-home';$env:CARGO_TARGET_DIR=$b+'\target-io-completion-tests'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location ($b+'\src')
$results=@(Get-Content -Raw -LiteralPath ($s+'\native-etw-notification-gates.json')|ConvertFrom-Json)
$results=@($results|Where-Object {$_.gate -ne 'capi'})
$retries=@()
foreach($step in @(@('capi','test --locked --offline --no-default-features -p rax-capi -- --nocapture --test-threads=1'),@('all-targets','build --locked --offline --no-default-features --all-targets'),@('integration','test --locked --offline --no-default-features --test user_windows_memory -- --nocapture'))){
 $name=$step[0];$command='cargo.exe '+$step[1];$log='native-etw-notification-'+$name+'.log'
 & cmd.exe /d /c ($command+' > '+$b+'\'+$log+' 2>&1');$gateExit=$LASTEXITCODE
 Copy-Item -LiteralPath ($b+'\'+$log) -Destination ($s+'\'+$log)
 $entry=@{gate=$name;'exit-code'=$gateExit;command=$command;log=$log}
 $results+=$entry;$retries+=$entry
 $results|ConvertTo-Json -Depth 5|Set-Content -Encoding utf8 -LiteralPath ($s+'\native-etw-notification-gates.json')
 $retries|ConvertTo-Json -Depth 5|Set-Content -Encoding utf8 -LiteralPath ($s+'\native-etw-notification-recovery-gates.json')
 Write-Output ('gate '+$name+' exit='+$gateExit)
 if($gateExit -ne 0){exit $gateExit}
}
exit 0
