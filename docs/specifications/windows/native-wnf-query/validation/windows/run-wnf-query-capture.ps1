$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
$rlib=$null
foreach($line in Get-Content ($b+'\native-access-check-final-build-user-artifacts.jsonl')){
 $j=$line|ConvertFrom-Json
 if($j.reason -eq 'compiler-artifact' -and $j.target.name -eq 'rax' -and $j.manifest_path -notlike '*\capi\Cargo.toml'){
  if($j.features.Count -ne 0){throw 'unexpected core features'}
  foreach($f in $j.filenames){if($f.EndsWith('.rlib')){$rlib=$f}}
 }
}
if(!$rlib){throw 'missing exact token core rlib'}
& rustc.exe --edition=2024 --crate-name native_wnf_query_trace ($s+'\native-wnf-query-trace.rs') --extern ('rax='+$rlib) -L ('dependency='+$b+'\target-io-completion-tests\debug\deps') -o ($b+'\native-wnf-query-trace.exe')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& ($b+'\native-wnf-query-trace.exe') ($b+'\src\tests\fixtures\user\windows\bin\arm64\smoke.exe') *> ($s+'\native-wnf-query-capture.log')
exit $LASTEXITCODE
