$ErrorActionPreference='Stop'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
& 'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe' ($s+'\native-timer2-repair-adapter.py')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
& ($s+'\native-timer2-gates.ps1')
exit $LASTEXITCODE
