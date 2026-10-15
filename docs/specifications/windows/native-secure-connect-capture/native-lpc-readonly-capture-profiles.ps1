@{pid=$PID;utc=[DateTime]::UtcNow.ToString('o')} | ConvertTo-Json | Set-Content -Encoding utf8 'C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154/native-lpc-readonly-capture-started.json'
$ErrorActionPreference = 'Stop'
$share = 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$outcomes = @()
foreach ($targetProfile in @('arm64','x64','x86','x86-laa')) {
    $architecture = if ($targetProfile -eq 'x86-laa') { 'x86' } else { $targetProfile }
    $directory = 'C:\Windows\Temp\assist-native-rax-20261010\lpc-readonly-capture-' + $targetProfile
    [IO.Directory]::CreateDirectory($directory) | Out-Null
    Copy-Item -LiteralPath ($share + '\native-lpc-readonly-capture.cpp') -Destination ($directory + '\native-lpc-readonly-capture.cpp')
    $sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath ($directory + '\native-lpc-readonly-capture.cpp')).Hash
    if ($sourceHash.ToLowerInvariant() -ne '15c627ae7087ebf9067d0cc0d68df91dfbc672ef9997dc96adc4bd9214937423') { throw 'source hash mismatch' }
    Set-Location $directory
    & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $architecture -HostArch arm64
    $largeAddress = if ($targetProfile -eq 'x86-laa') { ' /LARGEADDRESSAWARE' } else { '' }
    $stem = 'native-lpc-readonly-capture-' + $targetProfile
    & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /EHsc /we4473 native-lpc-readonly-capture.cpp /link /OUT:' + $stem + '.exe' + $largeAddress + ' > ' + $stem + '-build.log 2>&1')
    if ($LASTEXITCODE -ne 0) {
        Get-Content ($stem + '-build.log')
        exit $LASTEXITCODE
    }
    & cmd.exe /d /c ($stem + '.exe > ' + $stem + '.log 2>&1')
    $gateExit = $LASTEXITCODE
    foreach ($name in @(($stem + '.log'), ($stem + '-build.log'))) {
        Copy-Item -LiteralPath ($directory + '\' + $name) -Destination ($share + '\' + $name)
        Write-Output ($name + ' ' + (Get-FileHash -Algorithm SHA256 -LiteralPath ($directory + '\' + $name)).Hash)
    }
    $outcomes += @{'profile' = $targetProfile; 'exit-code' = $gateExit; 'source-hash' = $sourceHash; 'exe-sha256' = (Get-FileHash ($directory + '\' + $stem + '.exe')).Hash; 'exe-bytes' = (Get-Item ($directory + '\' + $stem + '.exe')).Length}
    if ($gateExit -ne 0) {
        $outcomes | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath ($share + '\native-lpc-readonly-capture-outcomes.json')
        exit $gateExit
    }
}
$outcomes | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath ($share + '\native-lpc-readonly-capture-outcomes.json')
