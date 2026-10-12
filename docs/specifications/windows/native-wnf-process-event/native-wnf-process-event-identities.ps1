$ErrorActionPreference='Stop'
$base='C:\Windows\Temp\assist-native-rax-20261010'
$share='C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154'
$identities=@();$sources=@();$images=@()
foreach($family in @('event','event-fresh','status')){
 foreach($profile in $(if($family -ne 'event'){@('arm64')}else{@('arm64','x64','x86','x86-laa')})){
  $stem=$(if($family -eq 'status'){'native-wnf-status-'+$profile}else{'native-wnf-process-'+$family+'-'+$profile})
  $directory=$(if($family -eq 'status'){$base+'\wnf-status-'+$profile}else{$base+'\wnf-process-'+$family+'-'+$profile})
  $source=$(if($family -eq 'status'){$directory+'\native-wnf-status.cpp'}else{$directory+'\native-wnf-process-'+$family+'.cpp'})
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
@{producer=$identities;sources=$sources;images=$images;dlls=$dlls;principal=@{name=$principal.Name;sid=$principal.User.Value};processor=$env:PROCESSOR_ARCHITECTURE;utc=[DateTime]::UtcNow.ToString('o')}|ConvertTo-Json -Depth 6|Set-Content -Encoding UTF8 -LiteralPath ($share+'\native-wnf-process-event-producer-identities.json')
