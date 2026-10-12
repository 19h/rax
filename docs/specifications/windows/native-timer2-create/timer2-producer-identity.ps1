$ErrorActionPreference='Stop'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
& 'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe' ($s+'\timer2-producer-identity.py')
if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
@{OSVersion=[Environment]::OSVersion.Version.ToString();Architecture=$env:PROCESSOR_ARCHITECTURE;ntdllVersion=(Get-Item 'C:\Windows\System32\ntdll.dll').VersionInfo.FileVersion;ntdllSha256=(Get-FileHash 'C:\Windows\System32\ntdll.dll').Hash;powershellVersion=$PSVersionTable.PSVersion.ToString()}|ConvertTo-Json|Set-Content -Encoding utf8 -LiteralPath ($s+'\timer2-native-host.json')
exit 0
