$ErrorActionPreference = 'Stop'
$task = 'C:\Windows\Temp\assist-native-rax-20261010'
$share = 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$directory = $task + '\apphelp-fresh-controls'
[IO.Directory]::CreateDirectory($directory) | Out-Null
Copy-Item -LiteralPath ($share + '\native-apphelp-fresh-controls.cpp') -Destination ($directory + '\native-apphelp-fresh-controls.cpp')
Set-Location $directory
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c 'cl.exe /nologo /std:c++20 /MT /we4473 native-apphelp-fresh-controls.cpp /link /OUT:native-apphelp-fresh-controls.exe > native-apphelp-fresh-controls-build.log 2>&1'
if ($LASTEXITCODE -ne 0) { Get-Content native-apphelp-fresh-controls-build.log; exit $LASTEXITCODE }
foreach ($mode in 0..7) {
    $env:APPHELP_CAPTURE_MODE = [string]$mode
    $name = 'native-apphelp-fresh-control-' + $mode + '.log'
    & cmd.exe /d /c ('native-apphelp-fresh-controls.exe > ' + $name + ' 2>&1')
    $gateExit = $LASTEXITCODE
    Copy-Item -LiteralPath ($directory + '\' + $name) -Destination ($share + '\' + $name)
    Write-Output ('case ' + $mode + ' exit ' + $gateExit)
    if ($gateExit -ne 0) { exit $gateExit }
}
Copy-Item -LiteralPath ($directory + '\native-apphelp-fresh-controls-build.log') -Destination ($share + '\native-apphelp-fresh-controls-build.log')
