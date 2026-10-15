$ErrorActionPreference = 'Stop'
$share = 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$outcomes = @()
foreach ($profile in @('arm64', 'x64', 'x86', 'x86-laa')) {
    $architecture = if ($profile -eq 'x86-laa') { 'x86' } else { $profile }
    $directory = 'C:\Windows\Temp\assist-native-rax-20261010\section-create-priority-' + $profile
    [IO.Directory]::CreateDirectory($directory) | Out-Null
    Copy-Item -LiteralPath ($share + '\native-section-create-priority.cpp') -Destination ($directory + '\native-section-create-priority.cpp')
    $sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath ($directory + '\native-section-create-priority.cpp')).Hash
    if ($sourceHash.ToLowerInvariant() -ne '2419bd1f00c559d87d828c41125972caf7cbe4b3b31c5f7ece7d06fd33ee412e') { throw 'source hash mismatch' }
    Set-Location $directory
    & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $architecture -HostArch arm64
    $largeAddress = if ($profile -eq 'x86-laa') { ' /LARGEADDRESSAWARE' } else { '' }
    $stem = 'native-section-create-priority-' + $profile
    & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /we4473 native-section-create-priority.cpp /link /OUT:' + $stem + '.exe' + $largeAddress + ' > ' + $stem + '-build.log 2>&1')
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
    $outcomes += @{'profile' = $profile; 'exit-code' = $gateExit; 'source-hash' = $sourceHash; 'exe-sha256' = (Get-FileHash ($directory + '\' + $stem + '.exe')).Hash; 'exe-bytes' = (Get-Item ($directory + '\' + $stem + '.exe')).Length}
    if ($gateExit -ne 0) {
        $outcomes | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath ($share + '\native-section-create-priority-outcomes.json')
        exit $gateExit
    }
}
$outcomes | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath ($share + '\native-section-create-priority-outcomes.json')
