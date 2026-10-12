$ErrorActionPreference='Stop'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
& whoami.exe /user /fo csv | Set-Content -LiteralPath ($share+'\native-object-directory-open-principal.csv') -Encoding UTF8
& 'C:\Windows\Temp\assist-native-rax-20261010\python\python.exe' ($share+'\native-object-directory-open-producer-identities.py')
exit $LASTEXITCODE
