$ErrorActionPreference='Stop'
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\native-wnf-process-event-current-overlay.ps1'
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\native-wnf-process-event-final-gates.ps1'
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& powershell.exe -NoProfile -ExecutionPolicy Bypass -File 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154\assist-native-wnf-process-event-owned.ps1'
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
