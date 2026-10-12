$ErrorActionPreference='Stop'
$base='C:\Windows\Temp\assist-native-rax-20261010'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$identities=@();$sources=@();$images=@()
foreach($family in @('binding','binding-sequence-boundaries')){
 foreach($profile in @('arm64','x64','x86','x86-laa')){
  $stem='native-worker-factory-'+$family+'-'+$profile
  $directory=$base+'\worker-factory-'+$family+'-'+$profile
  $source=$directory+'\native-worker-factory-'+$family+'.cpp'
  $item=Get-Item -LiteralPath $source
  $sources+=@{family=$family;profile=$profile;path=$source;bytes=$item.Length;sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $source).Hash}
  $image=[IO.File]::ReadAllBytes($directory+'\'+$stem+'.exe')
  $offset=[BitConverter]::ToUInt32($image,60)
  if([BitConverter]::ToUInt32($image,$offset) -ne 0x4550){throw 'PE signature'}
  $images+=@{family=$family;profile=$profile;machine=[BitConverter]::ToUInt16($image,$offset+4);characteristics=[BitConverter]::ToUInt16($image,$offset+22)}
  foreach($suffix in @('.exe','.log','-build.log')){
   $path=$directory+'\'+$stem+$suffix
   $item=Get-Item -LiteralPath $path
   $identities+=@{family=$family;profile=$profile;path=$path;bytes=$item.Length;sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash}
  }
 }
}
$principal=[Security.Principal.WindowsIdentity]::GetCurrent()
$dlls=@();foreach($path in @('C:\Windows\System32\ntdll.dll','C:\Windows\SysWOW64\ntdll.dll')){$item=Get-Item -LiteralPath $path;$dlls+=@{path=$path;bytes=$item.Length;sha256=(Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash;version=$item.VersionInfo.FileVersion}}
@{producer=$identities;sources=$sources;images=$images;dlls=$dlls;principal=@{name=$principal.Name;sid=$principal.User.Value};processor=$env:PROCESSOR_ARCHITECTURE;utc=[DateTime]::UtcNow.ToString('o')}|ConvertTo-Json -Depth 6|Set-Content -Encoding UTF8 -LiteralPath ($share+'\native-worker-factory-binding-producer-identities.json')
