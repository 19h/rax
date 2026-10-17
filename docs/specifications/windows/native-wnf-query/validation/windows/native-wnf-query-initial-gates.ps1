$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
& ($b+'\python\python.exe') -B ($s+'\apply-wnf-query-initial.py')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
$env:RUSTC=$b+'\toolchain\bin\rustc.exe'
$env:CARGO_HOME=$b+'\cargo-home'
$env:CARGO_TARGET_DIR=$b+'\target-io-completion-tests'
$env:CARGO_BUILD_JOBS='2'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location ($b+'\src')
$results=@()
$failed=$false
$steps=@(@('query','test --lib wnf::query_tests'),@('service','test --lib wnf_query_tests'),@('subscription','test --lib user::windows::wnf::tests'),@('access','test --lib access_check_tests'),@('all-targets','check --all-targets'))
foreach($step in $steps){
 $name=$step[0];$parts=$step[1].Split(' ',2);$command='cargo.exe '+$parts[0]+' --locked --offline --no-default-features '+$parts[1]
 if($name -ne 'all-targets'){$command+=' -- --nocapture --test-threads=1'}
 $log='native-wnf-query-initial-'+$name+'.log'
 & cmd.exe /d /c ($command+' > '+$b+'\'+$log+' 2>&1')
 $gateExit=$LASTEXITCODE
 Copy-Item -LiteralPath ($b+'\'+$log) -Destination ($s+'\'+$log)
 $results+=@{gate=$name;'exit-code'=$gateExit;command=$command;log=$log}
 $results|ConvertTo-Json -Depth 4|Set-Content -Encoding utf8 -LiteralPath ($s+'\native-wnf-query-initial-gates.json')
 Write-Output ('gate '+$name+' exit='+$gateExit)
 if($gateExit -ne 0){$failed=$true}
}
if($failed){exit 1}else{exit 0}
