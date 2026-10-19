$record=Get-Content -Raw 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\attributes-native-epoch2-gates.json'|ConvertFrom-Json
$parent=$record.'child-pid'
Get-CimInstance Win32_Process -Filter "ParentProcessId=$parent" | ForEach-Object {
 $item=Get-Process -Id $_.ProcessId
 [PSCustomObject]@{Id=$item.Id;Name=$item.ProcessName;CpuSeconds=$item.CPU;WorkingSetBytes=$item.WorkingSet64;Start=$item.StartTime.ToUniversalTime().ToString('o')}
} | ConvertTo-Json
