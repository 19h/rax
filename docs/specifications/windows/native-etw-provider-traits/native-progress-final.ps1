$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
Get-CimInstance Win32_Process | Where-Object {$_.Name -in @('cargo.exe','rustc.exe','cl.exe','link.exe','python.exe','cmd.exe','powershell.exe','native-etw-provider-traits-after-production-trace.exe','native-process-probe.exe')} | Select-Object ProcessId,Name,CommandLine | ConvertTo-Json -Depth 3
foreach($n in @('native-etw-provider-traits-full.log','assist-native-etw-provider-traits-owned-cpp.log','native-etw-provider-traits-after-production-trace-output.log')){Write-Output $n;if(Test-Path ($b+'\'+$n)){Get-Content -Tail 3 -LiteralPath ($b+'\'+$n)}}
