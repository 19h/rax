$ErrorActionPreference = 'Stop'
$share = 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$outcomes = @()
foreach ($profile in @('arm64', 'x64', 'x86', 'x86-laa')) {
    $architecture = if ($profile -eq 'x86-laa') { 'x86' } else { $profile }
    $directory = 'C:\Windows\Temp\assist-native-rax-20261010\token-query-v5-' + $profile
    [IO.Directory]::CreateDirectory($directory) | Out-Null
    Copy-Item -LiteralPath ($share + '\native-token-query-v5.cpp') -Destination ($directory + '\native-token-query-v5.cpp')
    $sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath ($directory + '\native-token-query-v5.cpp')).Hash
    if ($sourceHash.ToLowerInvariant() -ne '8b15c55550e7b33b3effdf7d8847f794794b71e944c524e0e6eec3976b7b732c') { throw 'source hash mismatch' }
    Set-Location $directory
    & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $architecture -HostArch arm64
    $largeAddress = if ($profile -eq 'x86-laa') { ' /LARGEADDRESSAWARE' } else { '' }
    $stem = 'native-token-query-v5-' + $profile
    & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /GS /we4473 native-token-query-v5.cpp advapi32.lib /link /SUBSYSTEM:CONSOLE,10.0 /OUT:' + $stem + '.exe' + $largeAddress + ' > ' + $stem + '-build.log 2>&1')
    $buildExit = $LASTEXITCODE
    Copy-Item -LiteralPath ($directory + '\' + $stem + '-build.log') -Destination ($share + '\' + $stem + '-build.log')
    if ($buildExit -ne 0) { Get-Content ($stem + '-build.log'); exit $buildExit }
    & cmd.exe /d /c ($stem + '.exe ' + $profile + ' > ' + $stem + '.log 2>&1')
    $gateExit = $LASTEXITCODE
    Copy-Item -LiteralPath ($directory + '\' + $stem + '.log') -Destination ($share + '\' + $stem + '.log')
    $outcomes += @{'profile' = $profile; 'exit-code' = $gateExit; 'source-hash' = $sourceHash; 'exe-sha256' = (Get-FileHash ($directory + '\' + $stem + '.exe')).Hash; 'exe-bytes' = (Get-Item ($directory + '\' + $stem + '.exe')).Length}
    $outcomes | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath ($share + '\native-token-query-v5-outcomes.json')
    if ($gateExit -ne 0) { exit $gateExit }
}
