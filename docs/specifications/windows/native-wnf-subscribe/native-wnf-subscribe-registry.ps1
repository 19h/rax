$ErrorActionPreference='Stop'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$path='Registry::HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Notifications'
$key=Get-Item -LiteralPath $path
$rows=@($key.GetValueNames()|Sort-Object|ForEach-Object { $v=$key.GetValue($_);[ordered]@{name=$_;kind=$key.GetValueKind($_).ToString();data=if($v -is [byte[]]){[BitConverter]::ToString($v).Replace('-','')}else{$v}} })
$rows|ConvertTo-Json -Depth 5|Set-Content -LiteralPath ($share+'\native-wnf-subscribe-registry.json')
Write-Output ('value-count='+$rows.Count)
$rows|Where-Object {$_.name -match '41911a28|10b5|ETW'}|ConvertTo-Json
