$ErrorActionPreference='Stop'
Get-MpThreatDetection | Where-Object {($_.Resources -join ' ') -like '*wnf-query-v5-x64.exe*'} | Select-Object ThreatID,ActionSuccess,InitialDetectionTime,LastThreatStatusChangeTime,Resources | ConvertTo-Json -Depth 5
