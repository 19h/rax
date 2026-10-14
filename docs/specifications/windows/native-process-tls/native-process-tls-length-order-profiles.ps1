$ErrorActionPreference = 'Stop'
$share = 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$outcomes = @()
foreach ($profile in @('arm64', 'x64', 'x86', 'x86-laa')) {
    $architecture = if ($profile -eq 'x86-laa') { 'x86' } else { $profile }
    $directory = 'C:\Windows\Temp\assist-native-rax-20261010\process-tls-length-order-' + $profile
    [IO.Directory]::CreateDirectory($directory) | Out-Null
    Copy-Item -LiteralPath ($share + '\native-process-tls-length-order.cpp') -Destination ($directory + '\native-process-tls-length-order.cpp')
    $sourceHash = (Get-FileHash -Algorithm SHA256 -LiteralPath ($directory + '\native-process-tls-length-order.cpp')).Hash
    if ($sourceHash.ToLowerInvariant() -ne '2f5981a378e4be56e270facd614918ba9b0bb0920e4acdf28a4712a7ae0ade1e') { throw 'source hash mismatch' }
    Set-Location $directory
    & 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch $architecture -HostArch arm64
    $largeAddress = if ($profile -eq 'x86-laa') { ' /LARGEADDRESSAWARE' } else { '' }
    $stem = 'native-process-tls-length-order-' + $profile
    & cmd.exe /d /c ('cl.exe /nologo /std:c++20 /MT /we4473 native-process-tls-length-order.cpp /link /OUT:' + $stem + '.exe' + $largeAddress + ' > ' + $stem + '-build.log 2>&1')
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
    if ($gateExit -ne 0 -and $gateExit -ne 22) {
        $outcomes | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath ($share + '\native-process-tls-length-order-outcomes.json')
        exit $gateExit
    }
}
$outcomes | ConvertTo-Json | Set-Content -Encoding utf8 -LiteralPath ($share + '\native-process-tls-length-order-outcomes.json')
