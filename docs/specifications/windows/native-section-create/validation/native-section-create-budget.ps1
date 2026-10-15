$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010';$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
Set-Location ($b+'\assist')
Copy-Item -LiteralPath ($s+'\native-section-create-budget-probe.cpp') -Destination 'native-section-create-budget-probe.cpp'
if((Get-FileHash 'native-section-create-budget-probe.cpp').Hash.ToLowerInvariant() -ne 'a20d74618d052932fdf0914f17832474871c8103b433030a2ce83429aa759398'){throw 'actual probe source'}
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT /EHsc /utf-8 /DNOMINMAX /Iinclude /Ivendor/rax/capi/include /Ijson native-section-create-budget-probe.cpp src/emulation/rax/rax_process.cpp ../target-io-completion-owned/debug/assist_rs.lib ws2_32.lib userenv.lib ntdll.lib bcrypt.lib advapi32.lib /Fe:native-cpp/native-section-create-budget-probe.exe > ../native-section-create-budget-build.log 2>&1'
if($LASTEXITCODE -ne 0){Get-Content ../native-section-create-budget-build.log;exit $LASTEXITCODE}
& cmd.exe /d /c 'native-cpp\native-section-create-budget-probe.exe > ../native-section-create-budget-output.log 2>&1'
$code=$LASTEXITCODE
foreach($n in @('native-section-create-budget-build.log','native-section-create-budget-output.log')) {Copy-Item -LiteralPath ($b+'\'+$n) -Destination ($s+'\'+$n)}
@{'exit-code'=$code;'source-sha256'=(Get-FileHash 'native-section-create-budget-probe.cpp').Hash;'exe-sha256'=(Get-FileHash 'native-cpp/native-section-create-budget-probe.exe').Hash;'exe-bytes'=(Get-Item 'native-cpp/native-section-create-budget-probe.exe').Length;'budget-ms-per-program'=60000;'max-turns-per-program'=1000000}|ConvertTo-Json|Set-Content -Encoding utf8 -LiteralPath ($s+'\native-section-create-budget-outcome.json')
exit $code
