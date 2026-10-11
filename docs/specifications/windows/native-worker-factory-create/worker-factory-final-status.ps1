$ErrorActionPreference='Stop'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
& 'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe' ($share+'\worker-factory-default-stack-identity.py')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
Get-CimInstance Win32_Process | Where-Object { $_.Name -match '^(cargo|rustc|rax-.*|user_windows_memory-.*)\.exe$' } | Select-Object Name,ProcessId,CommandLine | ConvertTo-Json -Depth 3
