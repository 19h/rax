$ErrorActionPreference = 'Stop'
$task = 'C:\Windows\Temp\assist-native-rax-20261010'
$share = 'C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$artifacts = Get-Content ($task + '\native-performance-counter-final-after-artifacts.jsonl') | ForEach-Object { $_ | ConvertFrom-Json }
$core = @($artifacts | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'rax' -and $_.manifest_path -notlike '*\capi\Cargo.toml' })
if ($core.Count -ne 1 -or @($core[0].features).Count -ne 0) { throw 'current empty-feature owning core required' }
$rlib = @($core[0].filenames | Where-Object { $_.EndsWith('.rlib') })[0]
$env:PATH = $task + '\toolchain\bin;' + $env:PATH
Copy-Item -LiteralPath ($share + '\native-process-tls-input-trace.rs') -Destination ($task + '\native-process-tls-input-trace.rs')
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
& cmd.exe /d /c ('rustc.exe --edition=2024 -C target-feature=+crt-static --crate-name native_production_diagnostic ' + $task + '\native-process-tls-input-trace.rs --extern "rax=' + $rlib + '" -L dependency=' + $task + '\target-io-completion-owned\debug\deps -o ' + $task + '\native-process-tls-input-trace.exe > ' + $task + '\native-process-tls-input-trace-build.log 2>&1')
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& cmd.exe /d /c ($task + '\native-process-tls-input-trace.exe ' + $task + '\src\tests\fixtures\user\windows\bin\arm64\smoke.exe > ' + $task + '\native-process-tls-input-trace-output.log 2>&1')
$gateExit = $LASTEXITCODE
foreach ($name in @('native-process-tls-input-trace-build.log', 'native-process-tls-input-trace-output.log')) {
    Copy-Item -LiteralPath ($task + '\' + $name) -Destination ($share + '\' + $name)
    Write-Output ($name + ' ' + (Get-FileHash -Algorithm SHA256 -LiteralPath ($task + '\' + $name)).Hash)
}
exit $gateExit
