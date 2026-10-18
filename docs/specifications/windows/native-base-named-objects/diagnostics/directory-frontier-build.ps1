$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
$rlib=$null
foreach($line in (Get-Content ($b+'\native-scalar-final-build-user-artifacts.jsonl'))){
 $j=$line|ConvertFrom-Json
 if($j.reason -eq 'compiler-artifact' -and $j.target.name -eq 'rax' -and $j.manifest_path -notlike '*\capi\Cargo.toml'){
  foreach($f in $j.filenames){if($f.EndsWith('.rlib')){$rlib=$f}}
 }
}
if(!$rlib){throw 'no current core RAX rlib'}
& ($b+'\toolchain\bin\rustc.exe') --edition=2024 --crate-name directory_frontier_trace ($s+'\directory-frontier-trace.rs') --extern ('rax='+$rlib) -L ('dependency='+$b+'\target-system-time-final\debug\deps') -o ($b+'\directory-frontier\directory-frontier-trace.exe')
exit $LASTEXITCODE
