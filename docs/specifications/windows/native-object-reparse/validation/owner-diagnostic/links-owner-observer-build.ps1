$ErrorActionPreference='Stop'
$b='C:\Windows\Temp\assist-native-rax-20261010'
$s='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$env:PATH=$b+'\toolchain\bin;'+$env:PATH
& 'C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch arm64 -HostArch arm64
$v=Get-Content ($s+'\native-links-final-owning-archive-identities.json') -Raw|ConvertFrom-Json
if($v.'core-features'.Count -ne 0){throw 'owner core features differ'}
$rlib=$v.archives|Where-Object {$_.path.EndsWith('.rlib')}|Select-Object -First 1
if(!$rlib){throw 'missing selected owner rlib'}
if((Get-FileHash $rlib.path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $rlib.sha256){throw 'owner archive changed'}
& ($b+'\toolchain\bin\rustc.exe') --edition=2024 --crate-name base_owner_observer ($s+'\links-owner-observer.rs') -C target-feature=+crt-static --extern ('rax='+$rlib.path) -L ('dependency='+$b+'\target-io-completion-owned\debug\deps') -o ($b+'\links-owner-observer.exe')
exit $LASTEXITCODE
