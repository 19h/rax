$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010';$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$active=Get-CimInstance Win32_Process | Where-Object {$_.Name -eq 'cargo.exe' -or $_.Name -eq 'rustc.exe'}
if($active){throw 'existing native Rust validation process; refusing duplicate launch'}
if((Get-Content -Raw -LiteralPath ($s+'\native-symbolic-link-focused-final.json') | ConvertFrom-Json).'exit-code' -ne 0){throw 'final focused native gate failed'}
$started=@()
foreach($name in @('native-symbolic-link-open-final-gates','assist-native-symbolic-link-open-final-owned')){
 $p=Start-Process -FilePath 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',($s+'\'+$name+'.ps1')) -RedirectStandardOutput ($b+'\'+$name+'-launcher-stdout.log') -RedirectStandardError ($b+'\'+$name+'-launcher-stderr.log') -PassThru
 $started+=@{script=$name;pid=$p.Id}
}
$started|ConvertTo-Json -Depth 4|Set-Content -Encoding utf8 -LiteralPath ($s+'\symbolic-link-final-validation-launches.json')
$started|ConvertTo-Json -Compress
