$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
& ($b+'\python\python.exe') -B ($s+'\apply-cookie-owner-final.py')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
$env:RUSTC=$b+'\toolchain\bin\rustc.exe'
$env:CARGO_HOME=$b+'\cargo-home'
$env:CARGO_TARGET_DIR=$b+'\target-io-completion-tests'
$env:CARGO_BUILD_JOBS='2'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location ($b+'\src')
& cmd.exe /d /c ('cargo.exe build --locked --offline --no-default-features --bin rax-user --message-format=json > '+$b+'\native-cookie-owner-final-build-user-artifacts.jsonl 2> '+$b+'\native-cookie-owner-final-build-user.log')
$exitCode=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-cookie-owner-final-build-user.log') -Destination ($s+'\native-cookie-owner-final-build-user.log')
if($exitCode -ne 0){exit $exitCode}
$rlib=$null
foreach($line in (Get-Content ($b+'\native-cookie-owner-final-build-user-artifacts.jsonl'))){
 $j=$line|ConvertFrom-Json
 if($j.reason -eq 'compiler-artifact' -and $j.target.name -eq 'rax' -and $j.manifest_path -notlike '*\capi\Cargo.toml'){
  foreach($f in $j.filenames){if($f.EndsWith('.rlib')){$rlib=$f}}
 }
}
if(!$rlib){throw 'no current core RAX rlib artifact'}
$observer=$b+'\native-shared-section-trace.rs'
if((Get-FileHash -Algorithm SHA256 -LiteralPath $observer).Hash.ToLowerInvariant() -ne '514fdbc99de6f562002aa8a198deb79e7f5cd012db058aa1460a0e5730ae37c0'){throw 'observer source changed'}
& cmd.exe /d /c ('rustc.exe --edition=2024 --crate-name native_console_owner_trace '+$observer+' --extern rax='+$rlib+' -L dependency='+$env:CARGO_TARGET_DIR+'\debug\deps -o '+$b+'\native-cookie-owner-final-trace.exe > '+$b+'\native-cookie-owner-final-observer-build.log 2>&1')
$exitCode=$LASTEXITCODE
Copy-Item -LiteralPath ($b+'\native-cookie-owner-final-observer-build.log') -Destination ($s+'\native-cookie-owner-final-observer-build.log')
if($exitCode -ne 0){exit $exitCode}
& ($b+'\python\python.exe') -B ($s+'\run-cookie-owner-final-observations.py')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
$results=@()
$steps=@(@('focused','test --lib user::windows::loader::cookie_tests'),@('prior-services','test --lib native_console_owner'),@('all-targets','check --all-targets'),@('integration','test --test user_windows --test user_windows_memory'),@('capi','test -p rax-capi'),@('full','test --lib'))
foreach($step in $steps){
 $name=$step[0];$parts=$step[1].Split(' ',2);$command='cargo.exe '+$parts[0]+' --locked --offline --no-default-features '+$parts[1]
 if($name -ne 'all-targets'){$command+=' -- --nocapture';if($name -eq 'capi' -or $name -eq 'full'){$command+=' --test-threads=1'}}
 $log='native-cookie-owner-final-'+$name+'.log'
 & cmd.exe /d /c ($command+' > '+$b+'\'+$log+' 2>&1')
 $gateExit=$LASTEXITCODE
 Copy-Item -LiteralPath ($b+'\'+$log) -Destination ($s+'\'+$log)
 $results+=@{gate=$name;'exit-code'=$gateExit;command=$command;log=$log}
 $results|ConvertTo-Json -Depth 4|Set-Content -Encoding utf8 -LiteralPath ($s+'\native-cookie-owner-final-gates.json')
 Write-Output ('gate '+$name+' exit='+$gateExit)
 if($gateExit -ne 0 -and $name -ne 'full'){exit $gateExit}
}
exit 0
