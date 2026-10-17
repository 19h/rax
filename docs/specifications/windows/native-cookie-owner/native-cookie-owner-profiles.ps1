$ErrorActionPreference = 'Stop'
$share = 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$outcomes = @()
foreach ($profile in @('arm64', 'x64', 'x86', 'x86-laa')) {
    $architecture = if ($profile -eq 'x86-laa') { 'x86' } else { $profile }
    $directory = 'C:\Windows\Temp\assist-native-rax-20261010\cookie-owner-' + $profile
    [IO.Directory]::CreateDirectory($directory) | Out-Null
    Copy-Item -LiteralPath ($share + '\native-cookie-owner.cpp') -Destination ($directory + '\native-cookie-owner.cpp')
    $sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath ($directory + '\native-cookie-owner.cpp')).Hash
    if ($sourceHash.ToLowerInvariant() -ne '84bd3ec255fa747f84c22150e8254f544379bdbf6bc53ce7d245ddd3593f9a7a') { throw 'source hash mismatch' }
    Set-Location $directory
    & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $architecture -HostArch arm64
    $largeAddress = if ($profile -eq 'x86-laa') { ' /LARGEADDRESSAWARE' } else { '' }
    $stem = 'native-cookie-owner-' + $profile
    & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /GS /we4473 native-cookie-owner.cpp /link /SUBSYSTEM:CONSOLE,10.0 /OUT:' + $stem + '.exe' + $largeAddress + ' > ' + $stem + '-build.log 2>&1')
    $buildExit = $LASTEXITCODE
    Copy-Item -LiteralPath ($directory + '\' + $stem + '-build.log') -Destination ($share + '\' + $stem + '-build.log')
    if ($buildExit -ne 0) { Get-Content ($stem + '-build.log'); exit $buildExit }
    & cmd.exe /d /c ($stem + '.exe ' + $profile + ' > ' + $stem + '.log 2>&1')
    $gateExit = $LASTEXITCODE
    Copy-Item -LiteralPath ($directory + '\' + $stem + '.log') -Destination ($share + '\' + $stem + '.log')
    $outcomes += @{'profile' = $profile; 'exit-code' = $gateExit; 'source-hash' = $sourceHash; 'exe-sha256' = (Get-FileHash ($directory + '\' + $stem + '.exe')).Hash; 'exe-bytes' = (Get-Item ($directory + '\' + $stem + '.exe')).Length}
    $outcomes | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath ($share + '\native-cookie-owner-outcomes.json')
    if ($gateExit -ne 0) { exit $gateExit }
}
