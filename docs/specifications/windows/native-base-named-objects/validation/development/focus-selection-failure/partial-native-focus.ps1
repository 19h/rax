$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010';$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$bin=$null
foreach($line in (Get-Content ($s+'\native-base-final-full.log'))){
 if($line -match 'Running unittests src/lib.rs \((.+\.exe)\)'){$bin=$Matches[1]}
}
if(!$bin){throw 'missing Cargo-selected full-test executable'}
$before=(Get-FileHash -Algorithm SHA256 $bin).Hash.ToLowerInvariant()
& $bin native_base_named_object --nocapture --test-threads=1
$code=$LASTEXITCODE
$after=(Get-FileHash -Algorithm SHA256 $bin).Hash.ToLowerInvariant()
if($before -ne $after){throw 'test executable changed'}
@{exe=$bin;'sha256-before'=$before;'sha256-after'=$after;'exit-code'=$code;'source-epoch'=2;selection='Same Cargo-selected test binary as the complete native library suite'}|ConvertTo-Json|Set-Content -Encoding utf8 ($s+'\partial-native-focus-run.json')
exit $code
