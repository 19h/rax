$ErrorActionPreference='Stop'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$p=Get-CimInstance Win32_Process -Filter 'ProcessId = 1524'
if($p){
 if($p.CommandLine -notlike '*native-timer2-current-overlay.py*'){throw 'task ownership mismatch'}
 Stop-Process -Id 1524 -Force
}
& 'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe' ($s+'\native-timer2-current-overlay-bundle.py')
exit $LASTEXITCODE
