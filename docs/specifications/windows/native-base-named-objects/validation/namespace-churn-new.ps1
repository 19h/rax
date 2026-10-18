$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010';$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
$rlib=$null
foreach($line in (Get-Content ($s+'\native-base-final-build-user-artifacts.jsonl'))){
 $j=$line|ConvertFrom-Json
 if($j.reason -eq 'compiler-artifact' -and $j.target.name -eq 'rax'){
  foreach($name in $j.filenames){if($name.EndsWith('.rlib')){$rlib=$name}}
 }
}
if(!$rlib){throw 'missing native baseline archive'}
if((Get-FileHash ($s+'\native-namespace-churn.rs')).Hash.ToLowerInvariant() -ne '507fda0612ad1a9a39c888bc55c0ba58b37dd07524eccf0eb83e219228ba8eb9'){throw 'source changed'}
(Get-FileHash $rlib)|ConvertTo-Json|Set-Content -Encoding utf8 ($s+'\namespace-churn-new-archive-before.json')
& rustc.exe --edition=2024 --crate-name namespace_churn ($s+'\native-namespace-churn.rs') --extern ('rax='+$rlib) -L ('dependency='+$b+'\target-system-time-final\debug\deps') -o ($b+'\namespace-churn-new.exe') 2>&1|Out-File -Encoding utf8 ($s+'\namespace-churn-new-build.log')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($b+'\namespace-churn-new.exe') arm64 > ($s+'\namespace-churn-new-exe-before.json')
& ($b+'\namespace-churn-new.exe') ($b+'\src-base-named-objects\tests\fixtures\user\windows\bin\arm64\smoke.exe') 2>&1|Out-File -Encoding utf8 ($s+'\namespace-churn-new.log')
$code=$LASTEXITCODE
(Get-FileHash $rlib)|ConvertTo-Json|Set-Content -Encoding utf8 ($s+'\namespace-churn-new-archive-after.json')
& ($b+'\python\python.exe') -B ($s+'\inspect-system-time-v2-pe.py') ($b+'\namespace-churn-new.exe') arm64 > ($s+'\namespace-churn-new-exe-after.json')
Write-Output ('churn baseline exit='+$code)
exit $code
